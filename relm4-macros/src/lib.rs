#![doc(
    html_logo_url = "https://raw.githubusercontent.com/AaronErhardt/relm4/main/assets/Relm_logo.svg"
)]
#![doc(
    html_favicon_url = "https://raw.githubusercontent.com/AaronErhardt/relm4/main/assets/Relm_logo.svg"
)]

use proc_macro::{self, TokenStream};
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned, ToTokens};
use syn::{parse_macro_input, spanned::Spanned, Error, GenericArgument, PathArguments};

mod additional_fields;
mod args;
mod attrs;
mod funcs;
mod item_impl;
mod macros;
mod types;
mod util;
mod widgets;

use attrs::Attrs;
use funcs::Funcs;
use item_impl::ItemImpl;
use macros::Macros;
use types::ModelTypes;

/// Macro that implemements [relm4::Widgets](https://aaronerhardt.github.io/docs/relm4/relm4/trait.Widgets.html) and generates the corresponding struct.
///
/// # Attributes
///
/// To create public struct use `#[widget(pub)]` or `#[widget(visibility = pub)]`.
///
/// If you use reexports to provide relm4, then you can use `#[widget(relm4= ::myreexports::my_relm)]` to override relm4 used during generating struct.
///
/// # Example
///
/// ```
/// use gtk::prelude::{BoxExt, ButtonExt, GtkWindowExt, OrientableExt};
/// use relm4::{send, AppUpdate, Model, RelmApp, Sender, WidgetPlus, Widgets};
///
/// #[derive(Default)]
/// struct AppModel {
///     counter: u8,
/// }
///
/// enum AppMsg {
///     Increment,
///     Decrement,
/// }
///
/// impl Model for AppModel {
///     type Msg = AppMsg;
///     type Widgets = AppWidgets;
///     type Components = ();
/// }
///
/// impl AppUpdate for AppModel {
///     fn update(&mut self, msg: AppMsg, _components: &(), _sender: Sender<AppMsg>) -> bool {
///         match msg {
///             AppMsg::Increment => {
///                 self.counter = self.counter.wrapping_add(1);
///             }
///             AppMsg::Decrement => {
///                 self.counter = self.counter.wrapping_sub(1);
///             }
///         }
///         true
///     }
/// }
///
/// #[relm4_macros::widget]
/// impl Widgets<AppModel, ()> for AppWidgets {
///     view! {
///         gtk::ApplicationWindow {
///             set_title: Some("Simple app"),
///             set_default_width: 300,
///             set_default_height: 100,
///             set_child = Some(&gtk::Box) {
///                 set_orientation: gtk::Orientation::Vertical,
///                 set_margin_all: 5,
///                 set_spacing: 5,
///
///                 append = &gtk::Button {
///                     set_label: "Increment",
///                     connect_clicked(sender) => move |_| {
///                         send!(sender, AppMsg::Increment);
///                     },
///                 },
///                 append = &gtk::Button {
///                     set_label: "Decrement",
///                     connect_clicked(sender) => move |_| {
///                         send!(sender, AppMsg::Decrement);
///                     },
///                 },
///                 append = &gtk::Label {
///                     set_margin_all: 5,
///                     set_label: watch! { &format!("Counter: {}", model.counter) },
///                 }
///             },
///         }
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn widget(attributes: TokenStream, input: TokenStream) -> TokenStream {
    let Attrs {
        visibility,
        relm4_path,
        ..
    } = parse_macro_input!(attributes as Attrs);
    let data = parse_macro_input!(input as ItemImpl);

    let trait_generics = if let PathArguments::AngleBracketed(generics) =
        &data.trait_.segments.last().unwrap().arguments
    {
        generics
    } else {
        return TokenStream::from(
            Error::new(
                data.trait_.segments.span(),
                "Expected generic parameters for model and parent model",
            )
            .to_compile_error(),
        );
    };

    let ModelTypes {
        model,
        parent_model,
    } = match ModelTypes::new(trait_generics) {
        Ok(model) => model,
        Err(err) => return TokenStream::from(err.to_compile_error()),
    };

    let trait_ = data.trait_;
    let ty = data.self_ty;
    let outer_attrs = &data.outer_attrs;

    // Find the type of the model

    // This can be unwrapped savely because the path must have at least one segement after parsing successful.
    let path_args = trait_
        .segments
        .last()
        .map(|segment| &segment.arguments)
        .unwrap();

    let model_ty_opt = if let PathArguments::AngleBracketed(angle_args) = path_args {
        if let Some(GenericArgument::Type(model_ty)) = angle_args.args.first() {
            Some(model_ty)
        } else {
            None
        }
    } else {
        None
    };

    let model_ty = if let Some(model_ty) = model_ty_opt {
        model_ty
    } else {
        return Error::new(
            path_args.span(),
            "Expected generic parameters for the model and the parent model",
        )
        .to_compile_error()
        .into();
    };

    let Macros {
        widgets,
        additional_fields,
    } = match Macros::new(&data.macros, data.brace_span.unwrap()) {
        Ok(macros) => macros,
        Err(err) => return TokenStream::from(err.to_compile_error()),
    };

    let Funcs {
        pre_init,
        post_init,
        pre_connect_components,
        post_connect_components,
        manual_view,
    } = match Funcs::new(&data.funcs) {
        Ok(macros) => macros,
        Err(err) => return TokenStream::from(err.to_compile_error()),
    };

    let root_widget_name = &widgets.name;
    let root_widget_type = widgets.func.type_token_stream();

    let mut widget_list = Vec::new();
    widgets.get_widget_list(&mut widget_list);

    let mut struct_stream = TokenStream2::new();
    let mut init_stream = TokenStream2::new();
    let mut return_stream = TokenStream2::new();
    let mut property_stream = TokenStream2::new();
    let mut view_stream = TokenStream2::new();
    let mut connect_stream = TokenStream2::new();
    let mut track_stream = TokenStream2::new();
    let mut component_stream = TokenStream2::new();
    let mut connect_component_stream = TokenStream2::new();

    for widget in widget_list {
        let w_name = &widget.name;
        let w_ty = widget.func.type_token_stream();
        let w_span = widget.func.span();
        let w_func = widget.func.func_token_stream();

        struct_stream.extend(quote_spanned! {
            w_span =>
            #[allow(missing_docs)]
            #visibility #w_name: #w_ty,
        });

        init_stream.extend(quote_spanned! {
            w_span => let #w_name = #w_func;
        });

        return_stream.extend(widget.return_stream());
        widget.property_assign_stream(&relm4_path, &mut property_stream);
        widget.view_stream(&relm4_path, &mut view_stream);
        connect_stream.extend(widget.connect_stream());
        track_stream.extend(widget.track_stream(model_ty));
        component_stream.extend(widget.component_stream());
        connect_component_stream.extend(widget.connect_component_stream());
    }

    let impl_generics = data.impl_generics;
    let where_clause = data.where_clause;

    // Extract identifiers from additional fields for struct initialization: "test: u8" => "test"
    let additional_fields_return_stream = if let Some(fields) = &additional_fields {
        let mut tokens = TokenStream2::new();
        for field in fields.inner.pairs() {
            tokens.extend(field.value().ident.to_token_stream());
            tokens.extend(quote! {,});
        }
        tokens
    } else {
        TokenStream2::new()
    };

    let out = quote! {
        #[allow(dead_code)]
        #outer_attrs
        #visibility struct #ty {
            #struct_stream
            #additional_fields
        }

        impl #impl_generics #trait_ for #ty #where_clause {
            type Root = #root_widget_type;

            /// Initialize the UI.
            fn init_view(model: &#model, parent_widgets: &<#parent_model as #relm4_path::Model>::Widgets, sender: #relm4_path::Sender<<#model as #relm4_path::Model>::Msg>) -> Self {
                #pre_init
                #init_stream
                #property_stream
                #connect_stream
                #post_init
                Self {
                    #return_stream
                    #additional_fields_return_stream
                }
            }

            fn connect_components(&self, model: &#model, components: &<#model as #relm4_path::Model>::Components) {
                #pre_connect_components
                #component_stream
                #connect_component_stream
                #post_connect_components
            }

            /// Return the root widget.
            fn root_widget(&self) -> Self::Root {
                self.#root_widget_name.clone()
            }

            /// Update the view to represent the updated model.
            fn view(&mut self, model: &#model, sender: #relm4_path::Sender<<#model as #relm4_path::Model>::Msg>) {
                #manual_view
                #view_stream
                #track_stream
            }
        }
    };

    out.into()
}
