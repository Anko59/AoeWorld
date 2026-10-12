//! Parse spans rather than counting braces in strings or comments.
use syn::{
    Attribute, Meta,
    spanned::Spanned,
    visit::{self, Visit},
};

fn mentions_test(meta: &Meta) -> bool {
    match meta {
        Meta::Path(path) => path.is_ident("test"),
        Meta::List(list) if !list.path.is_ident("not") => list
            .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
            .is_ok_and(|items| items.iter().any(mentions_test)),
        _ => false,
    }
}

fn test_cfg(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<Meta>()
                .is_ok_and(|meta| mentions_test(&meta))
    })
}

fn test_marker(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().segments.last().is_some_and(|segment| {
            let name = segment.ident.to_string();
            name.contains("test") && !name.starts_with("cfg")
        })
    })
}

#[derive(Default)]
struct Regions {
    ranges: Vec<(usize, usize)>,
    test_context: Option<usize>,
}

impl Regions {
    fn record(&mut self, attrs: &[Attribute], span: proc_macro2::Span) {
        let start = attrs
            .first()
            .map_or(span.start().line, |attr| attr.span().start().line);
        self.ranges.push((
            self.test_context.map_or(start, |line| line.min(start)),
            span.end().line,
        ));
    }
}

impl Regions {
    fn under_cfg(&mut self, attrs: &[Attribute], visit: impl FnOnce(&mut Self)) {
        let previous = self.test_context;
        if previous.is_none() && test_cfg(attrs) {
            self.test_context = attrs.first().map(|attr| attr.span().start().line);
        }
        visit(self);
        self.test_context = previous;
    }
}

impl<'ast> Visit<'ast> for Regions {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.under_cfg(&item.attrs, |visitor| {
            if visitor.test_context.is_some() && item.content.is_some() {
                visitor.record(&item.attrs, item.span());
            } else {
                visit::visit_item_mod(visitor, item);
            }
        });
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if test_marker(&item.attrs) {
            self.record(&item.attrs, item.span());
        } else {
            self.under_cfg(&item.attrs, |visitor| visit::visit_item_fn(visitor, item));
        }
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if test_marker(&item.attrs) {
            self.record(&item.attrs, item.span());
        } else {
            self.under_cfg(&item.attrs, |visitor| {
                visit::visit_impl_item_fn(visitor, item)
            });
        }
    }

    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        if test_marker(&item.attrs) {
            self.record(&item.attrs, item.span());
        } else {
            self.under_cfg(&item.attrs, |visitor| {
                visit::visit_trait_item_fn(visitor, item)
            });
        }
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        self.under_cfg(&item.attrs, |visitor| visit::visit_item_impl(visitor, item));
    }

    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        self.under_cfg(&item.attrs, |visitor| {
            visit::visit_item_trait(visitor, item)
        });
    }

    fn visit_item_const(&mut self, item: &'ast syn::ItemConst) {
        self.under_cfg(&item.attrs, |visitor| {
            visit::visit_item_const(visitor, item)
        });
    }

    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        self.under_cfg(&item.attrs, |visitor| {
            visit::visit_item_static(visitor, item)
        });
    }
}

pub(super) fn regions(text: &str) -> syn::Result<Vec<(usize, usize)>> {
    let file = syn::parse_file(text)?;
    let mut regions = Regions::default();
    regions.under_cfg(&file.attrs, |visitor| visitor.visit_file(&file));
    Ok(regions.ranges)
}
