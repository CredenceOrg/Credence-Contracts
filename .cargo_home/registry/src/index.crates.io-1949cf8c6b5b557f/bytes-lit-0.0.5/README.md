# bytes-lit
Creates byte arrays from literal values.

Currently supports integer literals of unbounded size.

## Example

Get a byte array given an integer value. Leading zeros in hex (`0x`) and binary
(`0b`) integer form are preserved.

```rust
let bytes = bytes!(0x00ed3f55dec47250a52a8c0bb7038e72fa6ffaae33562f77cd2b629ef7fd424d);
assert_eq!(bytes, [
    0, 237, 63, 85, 222, 196, 114, 80, 165, 42, 140, 11, 183, 3, 142, 114,
    250, 111, 250, 174, 51, 86, 47, 119, 205, 43, 98, 158, 247, 253, 66, 77,
]);
```

Get the minimum sized byte array given an integer value to capture the value.
Leading zeros are ignored.

```rust
let bytes = bytesmin!(0x00ed3f55dec47250a52a8c0bb7038e72fa6ffaae33562f77cd2b629ef7fd424d);
assert_eq!(bytes, [
    237, 63, 85, 222, 196, 114, 80, 165, 42, 140, 11, 183, 3, 142, 114,
    250, 111, 250, 174, 51, 86, 47, 119, 205, 43, 98, 158, 247, 253, 66, 77,
]);
```
