//! Python の random.Random と同じ乱数（メルセンヌ・ツイスタ MT19937）。
//!
//! 経年劣化の粒子の模様を旧版とそろえるために使う（旧版は random.Random(seed).randbytes）。

const N: usize = 624;
const M: usize = 397;
const MATRIX_A: u32 = 0x9908_b0df;
const UPPER_MASK: u32 = 0x8000_0000;
const LOWER_MASK: u32 = 0x7fff_ffff;

/// Python の random.Random と同じ乱数を出す。
pub struct PyRandom {
    state: [u32; N],
    index: usize,
}

impl PyRandom {
    /// random.Random(seed) と同じ状態にする（seed は 0 以上の整数）。
    pub fn new(seed: u64) -> Self {
        // Python は整数の seed を 32bit ずつに分けた配列（下位から）で init_by_array する
        let mut key = vec![seed as u32];
        if seed >> 32 != 0 {
            key.push((seed >> 32) as u32);
        }
        let mut random = Self { state: [0; N], index: N };
        random.init_by_array(&key);
        random
    }

    fn init_genrand(&mut self, seed: u32) {
        self.state[0] = seed;
        for i in 1..N {
            let previous = self.state[i - 1];
            self.state[i] = 1_812_433_253u32.wrapping_mul(previous ^ (previous >> 30)).wrapping_add(i as u32);
        }
        self.index = N;
    }

    fn init_by_array(&mut self, key: &[u32]) {
        self.init_genrand(19_650_218);
        let (mut i, mut j) = (1usize, 0usize);
        for _ in 0..N.max(key.len()) {
            let previous = self.state[i - 1];
            self.state[i] = (self.state[i] ^ (previous ^ (previous >> 30)).wrapping_mul(1_664_525))
                .wrapping_add(key[j])
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                self.state[0] = self.state[N - 1];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
        }
        for _ in 0..N - 1 {
            let previous = self.state[i - 1];
            self.state[i] = (self.state[i] ^ (previous ^ (previous >> 30)).wrapping_mul(1_566_083_941))
                .wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                self.state[0] = self.state[N - 1];
                i = 1;
            }
        }
        self.state[0] = 0x8000_0000;
    }

    fn generate(&mut self) {
        let mag = |y: u32| if y & 1 == 0 { 0 } else { MATRIX_A };
        for k in 0..N {
            let y = (self.state[k] & UPPER_MASK) | (self.state[(k + 1) % N] & LOWER_MASK);
            self.state[k] = self.state[(k + M) % N] ^ (y >> 1) ^ mag(y);
        }
        self.index = 0;
    }

    /// 32bit の乱数（Python の getrandbits(32) と同じ）。
    pub fn next_u32(&mut self) -> u32 {
        if self.index >= N {
            self.generate();
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    /// n バイトの乱数（Python の randbytes(n) と同じ）。
    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(n + 4);
        for _ in 0..n / 4 {
            out.extend_from_slice(&self.next_u32().to_le_bytes());
        }
        let rest = n % 4;
        if rest > 0 {
            // 足りない分は、乱数の上位のビットを使う
            let word = self.next_u32() >> (32 - 8 * rest);
            out.extend_from_slice(&word.to_le_bytes()[..rest]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_as_python() {
        // random.Random(19700101) の getrandbits(32) と randbytes(10)
        let mut r = PyRandom::new(19_700_101);
        assert_eq!([r.next_u32(), r.next_u32(), r.next_u32()], [1_369_637_406, 4_021_421_386, 1_559_968_181]);
        let mut r = PyRandom::new(19_700_101);
        assert_eq!(r.bytes(10), [30, 2, 163, 81, 74, 5, 178, 239, 251, 92]);
    }

    #[test]
    fn long_sequences_refill() {
        let mut r = PyRandom::new(1);
        let bytes = r.bytes(5000);
        assert_eq!(bytes.len(), 5000);
        assert!(bytes.iter().any(|&b| b != bytes[0]));
    }
}
