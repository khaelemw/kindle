//! AES-128, Kindle's own (FIPS-197; no crate): the games' archives encrypt ranges of their files with it
//! (ECB, a key per file in the archive's header).

use std::sync::OnceLock;

/// AES-128 (ECB, one block at a time).
pub struct Aes128 {
    round_keys: [[u8; 16]; 11],
}

struct AesTables {
    sbox: [u8; 256],
    inv: [u8; 256],
}

fn aes_tables() -> &'static AesTables {
    static TABLES: OnceLock<AesTables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut sbox = [0u8; 256];
        let (mut p, mut q) = (1u8, 1u8);
        loop {
            p ^= p << 1 ^ if p & 0x80 != 0 { 0x1B } else { 0 };
            q ^= q << 1;
            q ^= q << 2;
            q ^= q << 4;
            if q & 0x80 != 0 {
                q ^= 0x09;
            }
            sbox[p as usize] = q ^ q.rotate_left(1) ^ q.rotate_left(2) ^ q.rotate_left(3) ^ q.rotate_left(4) ^ 0x63;
            if p == 1 {
                break;
            }
        }
        sbox[0] = 0x63;
        let mut inv = [0u8; 256];
        for (i, &v) in sbox.iter().enumerate() {
            inv[v as usize] = i as u8;
        }
        AesTables { sbox, inv }
    })
}

fn xtime(a: u8) -> u8 {
    (a << 1) ^ if a & 0x80 != 0 { 0x1B } else { 0 }
}

fn gmul(mut a: u8, mut b: u8) -> u8 {
    let mut r = 0;
    while b != 0 {
        if b & 1 != 0 {
            r ^= a;
        }
        a = xtime(a);
        b >>= 1;
    }
    r
}

impl Aes128 {
    pub fn new(key: &[u8; 16]) -> Self {
        let sbox = &aes_tables().sbox;
        let mut w = [[0u8; 4]; 44];
        for (i, word) in w.iter_mut().take(4).enumerate() {
            word.copy_from_slice(&key[4 * i..4 * i + 4]);
        }
        let mut rcon = 1u8;
        for i in 4..44 {
            let mut t = w[i - 1];
            if i % 4 == 0 {
                t = [sbox[t[1] as usize] ^ rcon, sbox[t[2] as usize], sbox[t[3] as usize], sbox[t[0] as usize]];
                rcon = xtime(rcon);
            }
            for j in 0..4 {
                w[i][j] = w[i - 4][j] ^ t[j];
            }
        }
        let mut round_keys = [[0u8; 16]; 11];
        for (r, rk) in round_keys.iter_mut().enumerate() {
            for c in 0..4 {
                rk[4 * c..4 * c + 4].copy_from_slice(&w[4 * r + c]);
            }
        }
        Self { round_keys }
    }

    /// The state is column-major: byte `4 * column + row`.
    pub fn encrypt_block(&self, block: &mut [u8; 16]) {
        let sbox = &aes_tables().sbox;
        let mut s = *block;
        for (b, k) in s.iter_mut().zip(self.round_keys[0]) {
            *b ^= k;
        }
        for r in 1..11 {
            let mut shifted = [0u8; 16];
            for c in 0..4 {
                for row in 0..4 {
                    shifted[4 * c + row] = sbox[s[4 * ((c + row) % 4) + row] as usize];
                }
            }
            s = shifted;
            if r < 10 {
                for c in 0..4 {
                    let a = [s[4 * c], s[4 * c + 1], s[4 * c + 2], s[4 * c + 3]];
                    s[4 * c] = gmul(a[0], 2) ^ gmul(a[1], 3) ^ a[2] ^ a[3];
                    s[4 * c + 1] = a[0] ^ gmul(a[1], 2) ^ gmul(a[2], 3) ^ a[3];
                    s[4 * c + 2] = a[0] ^ a[1] ^ gmul(a[2], 2) ^ gmul(a[3], 3);
                    s[4 * c + 3] = gmul(a[0], 3) ^ a[1] ^ a[2] ^ gmul(a[3], 2);
                }
            }
            for (b, k) in s.iter_mut().zip(self.round_keys[r]) {
                *b ^= k;
            }
        }
        *block = s;
    }

    pub fn decrypt_block(&self, block: &mut [u8; 16]) {
        let inv = &aes_tables().inv;
        let mut s = *block;
        for (b, k) in s.iter_mut().zip(self.round_keys[10]) {
            *b ^= k;
        }
        for r in (0..10).rev() {
            let mut shifted = [0u8; 16];
            for c in 0..4 {
                for row in 0..4 {
                    shifted[4 * c + row] = s[4 * ((c + 4 - row) % 4) + row];
                }
            }
            for (i, b) in s.iter_mut().enumerate() {
                *b = inv[shifted[i] as usize] ^ self.round_keys[r][i];
            }
            if r > 0 {
                for c in 0..4 {
                    let a = [s[4 * c], s[4 * c + 1], s[4 * c + 2], s[4 * c + 3]];
                    s[4 * c] = gmul(a[0], 14) ^ gmul(a[1], 11) ^ gmul(a[2], 13) ^ gmul(a[3], 9);
                    s[4 * c + 1] = gmul(a[0], 9) ^ gmul(a[1], 14) ^ gmul(a[2], 11) ^ gmul(a[3], 13);
                    s[4 * c + 2] = gmul(a[0], 13) ^ gmul(a[1], 9) ^ gmul(a[2], 14) ^ gmul(a[3], 11);
                    s[4 * c + 3] = gmul(a[0], 11) ^ gmul(a[1], 13) ^ gmul(a[2], 9) ^ gmul(a[3], 14);
                }
            }
        }
        *block = s;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn aes_encrypts_the_fips_197_example() {
        let aes = Aes128::new(&hex("000102030405060708090a0b0c0d0e0f").try_into().unwrap());
        let mut block: [u8; 16] = hex("00112233445566778899aabbccddeeff").try_into().unwrap();
        aes.encrypt_block(&mut block);
        assert_eq!(block.to_vec(), hex("69c4e0d86a7b0430d8cdb78070b4c55a"));
        aes.decrypt_block(&mut block);
        assert_eq!(block.to_vec(), hex("00112233445566778899aabbccddeeff"));
    }

    #[test]
    fn aes_decrypts_the_fips_197_example() {
        let aes = Aes128::new(&hex("000102030405060708090a0b0c0d0e0f").try_into().unwrap());
        let mut block: [u8; 16] = hex("69c4e0d86a7b0430d8cdb78070b4c55a").try_into().unwrap();
        aes.decrypt_block(&mut block);
        assert_eq!(block.to_vec(), hex("00112233445566778899aabbccddeeff"));
    }
}
