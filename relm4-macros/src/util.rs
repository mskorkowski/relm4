use proc_macro::Span;
use syn::Ident;

pub(super) fn idents_to_snake_case(idents: &[Ident]) -> Ident {
    use std::sync::atomic::{AtomicU16, Ordering};
    static COUNTER: AtomicU16 = AtomicU16::new(0);
    let val = COUNTER.fetch_add(1, Ordering::Relaxed);
    let index_str = val.to_string();

    let segements: Vec<String> = idents
        .iter()
        .map(|ident| ident.to_string().to_lowercase())
        .collect();
    let length: usize =
        segements.iter().map(|seg| seg.len() + 1).sum::<usize>() + index_str.len() + 1;
    let mut name: String = String::with_capacity(length);

    for seg in &segements {
        name.push('_');
        name.push_str(seg);
    }
    name.push('_');
    name.push_str(&index_str);

    Ident::new(&name, Span::call_site().into())
}
