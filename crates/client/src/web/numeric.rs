//! Exact fixed-decimal UI text computed with browser-native integer arithmetic.
//!
//! Browser-native BigInt rounds the original IEEE-754 value once, nearest with
//! ties to even, just like Rust's fixed-precision formatter. This only changes
//! where readout strings are produced: camera values and authoritative integer
//! simulation coordinates are untouched. No byte saving is assumed until measured.
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(inline_js = "
const bits = new DataView(new ArrayBuffer(8));
export function set_coordinates(document, x, y) {
  const element = document.getElementById('world-position');
  if (element) element.textContent = exact_fixed(x, 1) + ', ' + exact_fixed(y, 1);
}
export function exact_fixed(value, digits) {
    if (!Number.isInteger(digits) || digits < 0 || digits > 20) {
        throw new RangeError('fixed precision must be an integer from 0 to 20');
    }
    bits.setFloat64(0, value, false);
    const raw = bits.getBigUint64(0, false);
    const negative = (raw >> 63n) !== 0n;
    const exponent = Number((raw >> 52n) & 2047n);
    let mantissa = raw & ((1n << 52n) - 1n);
    if (exponent === 2047) {
        return mantissa !== 0n ? 'NaN' : (negative ? '-inf' : 'inf');
    }
    const power = exponent === 0 ? -1074 : exponent - 1023 - 52;
    if (exponent !== 0) mantissa |= 1n << 52n;
    let rounded = mantissa * (10n ** BigInt(digits));
    if (power >= 0) {
        rounded <<= BigInt(power);
    } else {
        const denominator = 1n << BigInt(-power);
        const remainder = rounded % denominator;
        rounded /= denominator;
        const twice = remainder * 2n;
        if (twice > denominator || (twice === denominator && (rounded & 1n) !== 0n)) {
            rounded += 1n;
        }
    }
    let text = rounded.toString();
    if (digits !== 0) {
        text = text.padStart(digits + 1, '0');
        text = text.slice(0, -digits) + '.' + text.slice(-digits);
    }
    return (negative ? '-' : '') + text;
}
")]
extern "C" {
    #[wasm_bindgen(js_name = exact_fixed)]
    pub(crate) fn format(value: f64, digits: u32) -> String;
    pub(crate) fn set_coordinates(document: &web_sys::Document, x: f64, y: f64);
}

#[path = "numeric/tests.rs"]
#[cfg(test)]
mod tests;
