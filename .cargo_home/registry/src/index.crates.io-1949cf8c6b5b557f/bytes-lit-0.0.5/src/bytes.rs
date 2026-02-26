use std::{convert::TryInto, str::FromStr};

use num_bigint::BigUint;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Error, LitInt};

pub fn bytes(input: TokenStream2) -> TokenStream2 {
    let lit = match syn::parse2::<LitInt>(input) {
        Ok(lit) => lit,
        Err(e) => return e.to_compile_error(),
    };

    // Convert the integer literal into a base10 string, and into a slice of
    // bytes, via a big integer. The conversion should never fail because
    // syn::LitInt already validated the integer, unless the value is negative.
    // Any leading zeroes are discarded.
    let int = match BigUint::from_str(lit.base10_digits()) {
        Ok(int) => int,
        Err(_) => return Error::new(lit.span(), "negative values unsupported").to_compile_error(),
    };

    // Get the raw integer literal as it appears in the token stream.
    let raw = lit.to_string();

    // Remove underscores between digits.
    let normalized = raw.replace('_', "");

    // Remove any leading prefix that indicates the base, and use the base to
    // determine how many bits per leading zero needs to be prefilled into the
    // bytes generated. If bits_per_digit is None, leading zero digits are
    // unsupported.
    let (form, bits_per_zero_digit, remainder) = match normalized.as_bytes() {
        [b'0', b'x', r @ ..] => ("hex", Some(4), r),
        [b'0', b'b', r @ ..] => ("binary", Some(1), r),
        [b'0', b'o', r @ ..] => ("octal", None, r),
        [r @ ..] => ("decimal", None, r),
    };

    // Count the leading zero bits by counting the number of leading zeros and
    // multiplying by the bits per digit.
    let leading_zero_count = remainder.iter().take_while(|d| **d == b'0').count();
    let leading_zero_bits = if let Some(bits_per_digit) = bits_per_zero_digit {
        leading_zero_count
            .checked_mul(bits_per_digit)
            .expect("overflow")
    } else if leading_zero_count > 0 {
        // If there are leading zeros without a bits per digit error, since a
        // caller may expect the zeros to be preserved, and so it is better for
        // us to error. They can proceed by removing the zeros.
        return Error::new(
            lit.span(),
            format!(
                "leading zeros are not preserved or supported on integer literals in {} form",
                form,
            ),
        )
        .to_compile_error();
    } else {
        0
    };

    // Create the final byte slice, which has length of the leading zero bytes,
    // followed by the big integer bytes.
    let int_bits: usize = int.bits().try_into().expect("overflow");
    let int_bytes = int.to_bytes_be();
    let int_len = int_bytes.len();
    let total_bits = leading_zero_bits.checked_add(int_bits).expect("overflow");
    let total_len = (total_bits.checked_add(7).expect("overflow")) / 8;
    let mut total_bytes: Vec<u8> = vec![0; total_len];
    total_bytes[total_len - int_len..].copy_from_slice(&int_bytes);

    quote! { [#(#total_bytes),*] }
}

#[cfg(test)]
mod test {
    use super::bytes;
    use pretty_assertions::assert_eq;
    use proc_macro2::Span;
    use quote::quote;
    use syn::{parse_quote, Error, ExprArray};

    #[test]
    fn neg() {
        let tokens = bytes(quote! {-0x1});
        let expect = Error::new(Span::call_site(), "negative values unsupported")
            .to_compile_error()
            .to_string();
        assert_eq!(tokens.to_string(), expect);
    }

    #[test]
    fn hex() {
        let tokens = bytes(quote! {0x1});
        let parsed = syn::parse2::<ExprArray>(tokens).unwrap();
        let expect = syn::parse_quote!([1u8]);
        assert_eq!(parsed, expect);

        let tokens = bytes(quote! {0x928374892abc});
        let parsed = syn::parse2::<ExprArray>(tokens).unwrap();
        let expect = syn::parse_quote!([146u8, 131u8, 116u8, 137u8, 42u8, 188u8]);
        assert_eq!(parsed, expect);

        let tokens =
            bytes(quote! {0xfded3f55dec47250a52a8c0bb7038e72fa6ffaae33562f77cd2b629ef7fd424d});
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
        let tokens = bytes(quote! {340_282_366_920_938_463_463_374_607_431_768_211_455u128});
        let parsed = syn::parse2::<ExprArray>(tokens).unwrap();
        let expect = syn::parse_quote!([
            255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8, 255u8,
            255u8, 255u8, 255u8, 255u8
        ]);
        assert_eq!(parsed, expect);

        let tokens = bytes(quote! {340_282_366_920_938_463_463_374_607_431_768_211_456});
        let parsed = syn::parse2::<ExprArray>(tokens).unwrap();
        let expect = syn::parse_quote!([
            1u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8
        ]);
        assert_eq!(parsed, expect);
    }

    #[test]
    fn leading_zeros_preserved() {
        let table: &[(_, ExprArray)] = &[
            // Base 16.
            (quote!(0x1), parse_quote!([1u8])),
            (quote!(0x01), parse_quote!([1u8])),
            (quote!(0x0001), parse_quote!([0u8, 1u8])),
            (quote!(0x0_0_0_1), parse_quote!([0u8, 1u8])),
            (quote!(0x1u32), parse_quote!([1u8])),
            (quote!(0x01u32), parse_quote!([1u8])),
            (quote!(0x0001u32), parse_quote!([0u8, 1u8])),
            (quote!(0x0_0_0_1u32), parse_quote!([0u8, 1u8])),
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
            (quote!(0b000000001), parse_quote!([0u8, 1u8])),
        ];
        for (i, t) in table.iter().cloned().enumerate() {
            let tokens = bytes(t.0);
            let parsed = syn::parse2::<ExprArray>(tokens).unwrap();
            let expect = t.1;
            assert_eq!(parsed, expect, "table entry: {}", i);
        }
    }

    #[test]
    fn leading_zeros_prohibited() {
        let table: &[(_, Result<ExprArray, Error>)] = &[
            // Base 8.
            (quote!(0o377), Ok(parse_quote!([255u8]))),
            (quote!(0o0377), Err(Error::new(Span::call_site(), "leading zeros are not preserved or supported on integer literals in octal form"))),
            (quote!(0o00377), Err(Error::new(Span::call_site(), "leading zeros are not preserved or supported on integer literals in octal form"))),
            (quote!(0o400), Ok(parse_quote!([1u8, 0u8]))),
            // Base 10.
            (quote!(255), Ok(parse_quote!([255u8]))),
            (quote!(0255), Err(Error::new(Span::call_site(), "leading zeros are not preserved or supported on integer literals in decimal form"))),
            (quote!(00255), Err(Error::new(Span::call_site(), "leading zeros are not preserved or supported on integer literals in decimal form"))),
            (quote!(256), Ok(parse_quote!([1u8, 0u8]))),
        ];
        for (i, t) in table.iter().enumerate() {
            let tokens = bytes(t.0.clone());
            match t.1.clone() {
                Ok(expect) => {
                    let parsed = syn::parse2::<ExprArray>(tokens);
                    assert_eq!(parsed.unwrap(), expect, "table entry: {}", i);
                }
                Err(e) => {
                    assert_eq!(
                        tokens.to_string(),
                        e.to_compile_error().to_string(),
                        "table entry: {}",
                        i
                    );
                }
            };
        }
    }
}
