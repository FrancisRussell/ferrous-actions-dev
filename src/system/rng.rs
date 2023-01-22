use blake3::Hasher;
use derivative::Derivative;

const U8_BIT: usize = 8;

// Even this is probably too high: https://v8.dev/blog/math-random
const ENTROPY_BITS_PER_F64: usize = 32;

// To use rand/getrandom we either need node.js's crypto module to be enabled,
// or switch to WASI as a target

#[derive(Derivative)]
#[derivative(Debug)]
#[derive(Clone)]
pub struct MathRandom {
    #[derivative(Debug = "ignore")]
    bytes: [u8; 32],

    #[derivative(Debug = "ignore")]
    taken: usize,

    #[derivative(Debug = "ignore")]
    hasher: Hasher,
}

impl Default for MathRandom {
    fn default() -> MathRandom {
        MathRandom {
            bytes: [0u8; 32],
            taken: usize::MAX,
            hasher: Hasher::new(),
        }
    }
}

impl MathRandom {
    pub fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut written = 0;
        while written < dest.len() {
            if self.taken >= self.bytes.len() {
                self.refill();
            }
            let to_take = core::cmp::min(self.bytes.len() - self.taken, dest.len() - written);
            dest[written..(written + to_take)].copy_from_slice(&self.bytes[self.taken..(self.taken + to_take)]);
            self.taken += to_take;
            written += to_take;
        }
    }

    fn refill(&mut self) {
        let num_doubles_to_input = (self.bytes.len() * U8_BIT + ENTROPY_BITS_PER_F64 - 1) / ENTROPY_BITS_PER_F64;
        for _ in 0..num_doubles_to_input {
            let random = ffi::MATH
                .random()
                .as_f64()
                .expect("Math.random() didn't return a float");
            #[allow(clippy::cast_precision_loss)]
            let random = random * ((1u64 << f64::MANTISSA_DIGITS) as f64);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let random = random as u64;
            self.hasher.update(&random.to_le_bytes());
        }
        self.bytes = self.hasher.finalize().into();
        self.taken = 0;
    }
}

mod ffi {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        pub type MathObject;

        #[wasm_bindgen(js_name = "Math")]
        pub static MATH: MathObject;

        #[wasm_bindgen(method)]
        pub fn random(this: &MathObject) -> JsValue;
    }
}
