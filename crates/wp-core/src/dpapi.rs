//! 비밀 글(AI 키 · OBS 비밀번호) 보관: Windows DPAPI (이 PC·이 사용자만 풀 수 있음) + base64
//! 저장 모양: "dp1:<base64>". Electron 판 safeStorage 로 만든 keyEnc 는 풀 수 없다 (다시 입력 필요).

const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64(data: &[u8]) -> String {
    let mut out = String::new();
    for ch in data.chunks(3) {
        let b = [ch[0], *ch.get(1).unwrap_or(&0), *ch.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        for i in 0..4 {
            if i <= ch.len() {
                out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn b64url(data: &[u8]) -> String {
    b64(data).replace('+', "-").replace('/', "_").trim_end_matches('=').to_string()
}

pub fn unb64(s: &str) -> Option<Vec<u8>> {
    let mut bits = 0u32;
    let mut n = 0;
    let mut out = vec![];
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\r' | b'\n' | b' ' => continue,
            _ => return None,
        } as u32;
        bits = (bits << 6) | v;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((bits >> n) as u8);
            bits &= (1 << n) - 1;
        }
    }
    Some(out)
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB};

    fn blob(d: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB { cbData: d.len() as u32, pbData: d.as_ptr() as *mut u8 }
    }
    unsafe fn take(b: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        let v = unsafe { std::slice::from_raw_parts(b.pbData, b.cbData as usize).to_vec() };
        unsafe { LocalFree(b.pbData as _) };
        v
    }
    pub fn protect(d: &[u8]) -> Option<Vec<u8>> {
        let input = blob(d);
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        let ok = unsafe { CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out) };
        if ok == 0 { None } else { Some(unsafe { take(out) }) }
    }
    pub fn unprotect(d: &[u8]) -> Option<Vec<u8>> {
        let input = blob(d);
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        let ok = unsafe { CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut out) };
        if ok == 0 { None } else { Some(unsafe { take(out) }) }
    }
}

#[cfg(not(windows))]
mod imp {
    // 윈도우가 아니면 보호 없이 (맥 키체인은 다음 단계)
    pub fn protect(d: &[u8]) -> Option<Vec<u8>> { Some(d.to_vec()) }
    pub fn unprotect(d: &[u8]) -> Option<Vec<u8>> { Some(d.to_vec()) }
}

/// 비밀 글 → "dp1:…" (빈 글은 빈 글)
pub fn encrypt(plain: &str) -> String {
    if plain.is_empty() {
        return String::new();
    }
    imp::protect(plain.as_bytes()).map(|b| format!("dp1:{}", b64(&b))).unwrap_or_default()
}

/// "dp1:…" → 비밀 글 (못 풀면 None — Electron 판의 옛 값 등)
pub fn decrypt(enc: &str) -> Option<String> {
    let b = unb64(enc.strip_prefix("dp1:")?)?;
    String::from_utf8(imp::unprotect(&b)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn base64() {
        assert_eq!(b64(b"hello"), "aGVsbG8=");
        assert_eq!(unb64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(b64url(&[0xfb, 0xff]), "-_8");
    }
    #[test]
    fn roundtrip() {
        let e = encrypt("sk-or-비밀");
        assert!(e.starts_with("dp1:"));
        assert_eq!(decrypt(&e).as_deref(), Some("sk-or-비밀"));
        assert!(decrypt("v10abc").is_none());
    }
}
