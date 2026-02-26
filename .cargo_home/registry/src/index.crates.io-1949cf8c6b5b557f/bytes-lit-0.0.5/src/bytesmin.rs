use std::str::FromStr;

use num_bigint::BigUint;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Error, LitInt};

pub fn bytesmin(input: TokenStream2) -> TokenStream2 {
    let lit = match syn::parse2::<LitInt>(input) {
        Ok(lit) => lit,
        Err(e) => return e.to_compile_error(),
    };
    let int = match BigUint::from_str(lit.base10_digits()) {
        Ok(int) => int,
        Err(_) => return Error::new(lit.span(), "negative values unsupported").to_compile_error(),
    };
    let bytes = int.to_bytes_be();
    quote! { [#(#bytes),*] }
}

#[cfg(test)]
mod test {
    use super::bytesmin;
    use pretty_assertions::assert_eq;
    use proc_macro2::Span;
    use quote::quote;
    use syn::{parse_quote, Error, ExprArray};

    #[test]
    fn neg() {
        let tokens = bytesmin(quote! {-0x1});
        let expect = Error::new(Span::call_site(), "negative values unsupported")
            .to_compile_error()
            .to_string();
        assert_eq!(tokens.to_string(), expect);
    }

    #[test]
    fn hex() {
        let tokens = bytesmin(quote! {0x1});
        let parsed = syn::parse2::<ExprArray>(tokens).unwrap();
        let expect = syn::parse_quote!([1u8]);
        assert_eq!(parsed, expect);

        let tokens = bytesmin(quote! {0x928374892abc});
        let parsed = syn::parse2::<ExprArray>(tokens).unwrap();
        let expect = syn::parse_quote!([146u8, 131u8, 116u8, 137u8, 42u8, 188u8]);
        assert_eq!(parsed, expect);

        let tokens =
            bytesmin(quote! {0xfded3f55dec47250a52a8c0bb7038e72fa6ffaae33562f77cd2b629ef7fd424d});
        let parsed = syn::parse2::<ExprArray>(tokens).unwrap();
        let expect = syn::parse_quote!([
            253u8, 237u8, 63u8, 85u8, 222u8, 196u8, 114u8, 80u8, 165u8, 42u8, 140u8, 11u8, 183u8,
            3u8, 142u8, 114u8, 250u8, 111u8, 250u8, 174u8, 51u8, 86u8, 47u8, 119u8, 205u8, 43u8,
            98u8, 158u8, 247u8, 253u8, 66u8, 77u8
        ]);
        assert_eq!(parsed, expect);
    }

    #[test]
    fn base10() {
        let tokens = bytesmin(quote! {340_282_366_920_938_463_463_374_607_431_768_211_455u128});
        let parsed = syn::parse2::<ExprArray>(tokens).unwrap();
        let expect = syn::parse_quote!([
            255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8,
            255u8, 255u8, 255u8, 255u8
        ]);
        assert_eq!(parsed, expect);

        let tokens = bytesmin(quote! {340_282_366_920_938_463_463_374_607_431_768_211_456});
        let parsed = syn::parse2::<ExprArray>(tokens).unwrap();
        let expect = syn::parse_quote!([
            1u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8
        ]);
        assert_eq!(parsed, expect);
    }

    #[test]
    fn leading_zeros_discarded() {
        let table: &[(_, ExprArray)] = &[
            // Base 16.
            (quote!(0x1), parse_quote!([1u8])),
            (quote!(0x01), parse_quote!([1u8])),
            (quote!(0x0001), parse_quote!([1u8])),
            (quote!(0x0_0_0_1), parse_quote!([1u8])),
            (quote!(0x1u32), parse_quote!([1u8])),
            (quote!(0x01u32), parse_quote!([1u8])),
            (quote!(0x0001u32), parse_quote!([1u8])),
            (quote!(0x0_0_0_1u32), parse_quote!([1u8])),
            // Base 2.
            (quote!(0b1), parse_quote!([1u8])),
            (quote!(0b11), parse_quote!([3u8])),
            (quote!(0b111), parse_quote!([7u8])),
            (quote!(0b1111), parse_quote!([15u8])),
            (quote!(0b11111), parse_quote!([31u8])),
            (quote!(0b111111), parse_quote!([63u8])),
            (quote!(0b1111111), parse_quote!([127u8])),
            (quote!(0b11111111), parse_quote!([255u8])),
            (quote!(0b111111111), parse_quote!([1u8, 255u8])),
            (quote!(0b1), parse_quote!([1u8])),
            (quote!(0b01), parse_quote!([1u8])),
            (quote!(0b001), parse_quote!([1u8])),
            (quote!(0b0001), parse_quote!([1u8])),
            (quote!(0b00001), parse_quote!([1u8])),
            (quote!(0b000001), parse_quote!([1u8])),
            (quote!(0b0000001), parse_quote!([1u8])),
            (quote!(0b00000001), parse_quote!([1u8])),
            (quote!(0b000000001), parse_quote!([1u8])),
            // Base 8.
            (quote!(0o377), parse_quote!([255u8])),
            (quote!(0o0377), parse_quote!([255u8])),
            (quote!(0o00377), parse_quote!([255u8])),
            (quote!(0o400), parse_quote!([1u8, 0u8])),
            // Base 10.
            (quote!(255), parse_quote!([255u8])),
            (quote!(0255), parse_quote!([255u8])),
            (quote!(00255), parse_quote!([255u8])),
            (quote!(256), parse_quote!([1u8, 0u8])),
        ];
        for (i, t) in table.iter().cloned().enumerate() {
            let tokens = bytesmin(t.0);
            let parsed = syn::parse2::<ExprArray>(tokens).unwrap();
            let expect = t.1;
            assert_eq!(parsed, expect, "table entry: {}", i);
        }
    }
}
