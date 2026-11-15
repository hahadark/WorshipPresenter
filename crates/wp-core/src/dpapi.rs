//! 비밀 글(AI 키 · OBS 비밀번호) 보관: Windows DPAPI (이 PC·이 사용자만 풀 수 있음) + base64
//! 저장 모양: "dp1:<base64>". Electron 판 safeStorage 로 만든 값("v10"+AES-256-GCM, 열쇠는 Electron 의 Local State 에 DPAPI 로 보관)은
//! 같은 PC·같은 사용자면 풀어서 dp1 로 바꿔 이어받는다 (adopt_electron_secrets). 못 풀면 그대로 두고 다시 입력받는다.

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
    /// AES-256-GCM 풀기 (윈도우 CNG — 새 크레이트 없이). 꼬리표(tag)가 안 맞으면 None
    pub fn aes_gcm_decrypt(key: &[u8], nonce: &[u8], ct: &[u8], tag: &[u8]) -> Option<Vec<u8>> {
        use windows_sys::Win32::Security::Cryptography::*;
        let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        unsafe {
            let mut alg: BCRYPT_ALG_HANDLE = std::ptr::null_mut();
            if BCryptOpenAlgorithmProvider(&mut alg, w("AES").as_ptr(), std::ptr::null(), 0) != 0 {
                return None;
            }
            let mode = w("ChainingModeGCM");
            let mut out = None;
            if BCryptSetProperty(alg as _, w("ChainingMode").as_ptr(), mode.as_ptr() as *const u8, (mode.len() * 2) as u32, 0) == 0 {
                let mut hkey: BCRYPT_KEY_HANDLE = std::ptr::null_mut();
                if BCryptGenerateSymmetricKey(alg, &mut hkey, std::ptr::null_mut(), 0, key.as_ptr(), key.len() as u32, 0) == 0 {
                    let mut info: BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO = std::mem::zeroed();
                    info.cbSize = std::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as u32;
                    info.dwInfoVersion = 1;
                    info.pbNonce = nonce.as_ptr() as *mut u8;
                    info.cbNonce = nonce.len() as u32;
                    info.pbTag = tag.as_ptr() as *mut u8;
                    info.cbTag = tag.len() as u32;
                    let mut plain = vec![0u8; ct.len().max(1)];
                    let mut n = 0u32;
                    let st = BCryptDecrypt(hkey, ct.as_ptr(), ct.len() as u32, &info as *const _ as *const core::ffi::c_void, std::ptr::null_mut(), 0, plain.as_mut_ptr(), ct.len() as u32, &mut n, 0);
                    if st == 0 {
                        plain.truncate(n as usize);
                        out = Some(plain);
                    }
                    BCryptDestroyKey(hkey);
                }
            }
            BCryptCloseAlgorithmProvider(alg, 0);
            out
        }
    }
}

#[cfg(not(windows))]
mod imp {
    // 윈도우가 아니면 보호 없이 (맥 키체인은 다음 단계)
    pub fn protect(d: &[u8]) -> Option<Vec<u8>> { Some(d.to_vec()) }
    pub fn unprotect(d: &[u8]) -> Option<Vec<u8>> { Some(d.to_vec()) }
    pub fn aes_gcm_decrypt(_key: &[u8], _nonce: &[u8], _ct: &[u8], _tag: &[u8]) -> Option<Vec<u8>> { None }
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

/// Electron 판 safeStorage 값(base64 한 "v10"+nonce 12+암호문+tag 16) 풀기. local_state = Electron userData 의 "Local State"
pub fn electron_decrypt(enc_b64: &str, local_state: &std::path::Path) -> Option<String> {
    let data = unb64(enc_b64)?;
    if data.len() < 3 + 12 + 16 || &data[..3] != b"v10" {
        return None;
    }
    let ls: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(local_state).ok()?).ok()?;
    let ek = unb64(ls["os_crypt"]["encrypted_key"].as_str()?)?;
    let key = imp::unprotect(ek.strip_prefix(b"DPAPI")?)?;
    let (nonce, rest) = data[3..].split_at(12);
    let (ct, tag) = rest.split_at(rest.len() - 16);
    String::from_utf8(imp::aes_gcm_decrypt(&key, nonce, ct, tag)?).ok()
}

/// 1.x 에서 가져온 설정의 비밀 글(AI 키 · OBS 비밀번호)이 Electron 방식으로 잠겨 있으면 풀어서 이 판 방식(dp1)으로 바꾼다.
/// 전에는 풀지 못해 조용히 '키 없음'이 되었다. 바꾼 개수를 돌려준다 (값은 로그에 남기지 않는다)
pub fn adopt_electron_secrets(settings: &mut crate::settings::Settings, legacy_data: Option<&std::path::Path>) -> usize {
    let Some(old) = legacy_data else { return 0 };
    // 설치판: %APPDATA%\WorshipCue\Local State (자료는 그 아래 data\) · --data 실행: <자료>\electron\Local State
    let states: Vec<std::path::PathBuf> = [old.parent().map(|p| p.join("Local State")), Some(old.join("electron").join("Local State"))].into_iter().flatten().filter(|p| p.is_file()).collect();
    if states.is_empty() {
        return 0;
    }
    let mut found: Vec<(&str, &str, String)> = vec![];
    for (sec, key) in [("ai", "keyEnc"), ("obs", "passwordEnc")] {
        let Some(enc) = settings.get()[sec][key].as_str().filter(|e| !e.is_empty() && !e.starts_with("dp1:")) else { continue };
        if let Some(plain) = states.iter().find_map(|ls| electron_decrypt(enc, ls)) {
            let e = encrypt(&plain);
            if !e.is_empty() {
                found.push((sec, key, e));
            }
        }
    }
    let n = found.len();
    if n > 0 {
        settings.update(|d| {
            for (sec, key, e) in found {
                d[sec][key] = serde_json::json!(e);
            }
        });
        log::info!("1.x 판에서 잠가 둔 비밀 글 {n}개(AI 키·OBS 비밀번호)를 이 판 방식으로 바꿔 이어받았습니다");
    }
    n
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
    fn hexv(s: &str) -> Vec<u8> {
        (0..s.len() / 2).map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap()).collect()
    }
    /// NIST AES-256-GCM 시험 벡터 (열쇠·nonce 0, 평문 0 16바이트)
    #[cfg(windows)]
    #[test]
    fn aes_gcm_known_vector() {
        let (key, nonce) = ([0u8; 32], [0u8; 12]);
        let ct = hexv("cea7403d4d606b6e074ec5d3baf39d18");
        let tag = hexv("d0d1c8a799996bf0265b98b5d48ab919");
        assert_eq!(imp::aes_gcm_decrypt(&key, &nonce, &ct, &tag).unwrap(), vec![0u8; 16]);
        let mut bad = tag.clone();
        bad[0] ^= 1;
        assert!(imp::aes_gcm_decrypt(&key, &nonce, &ct, &bad).is_none(), "꼬리표가 틀리면 풀지 않는다");
    }
    /// Electron 방식으로 잠근 값을 흉내 내어(열쇠 0 을 DPAPI 로 싸서 Local State 에) 설정째 이어받는지
    #[cfg(windows)]
    #[test]
    fn adopt_electron_style_secret() {
        let d = std::env::temp_dir().join(format!("wp-esec-{}", rand::random::<u64>()));
        let old = d.join("WorshipCue").join("data");
        std::fs::create_dir_all(&old).unwrap();
        let mut ek = b"DPAPI".to_vec();
        ek.extend(imp::protect(&[0u8; 32]).unwrap());
        std::fs::write(d.join("WorshipCue").join("Local State"), serde_json::json!({ "os_crypt": { "encrypted_key": b64(&ek) } }).to_string()).unwrap();
        // "v10" + nonce(0×12) + 암호문 + tag  → 평문은 0 이 16개
        let mut blob = b"v10".to_vec();
        blob.extend([0u8; 12]);
        blob.extend(hexv("cea7403d4d606b6e074ec5d3baf39d18"));
        blob.extend(hexv("d0d1c8a799996bf0265b98b5d48ab919"));
        let new = d.join("new");
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("settings.json"), serde_json::json!({ "ai": { "keyEnc": b64(&blob) }, "obs": { "passwordEnc": "dp1:already" } }).to_string()).unwrap();
        let mut s = crate::settings::Settings::load(&new);
        assert_eq!(adopt_electron_secrets(&mut s, Some(&old)), 1);
        let e = s.get()["ai"]["keyEnc"].as_str().unwrap().to_string();
        assert!(e.starts_with("dp1:"));
        assert_eq!(decrypt(&e).unwrap().as_bytes(), &[0u8; 16]);
        assert_eq!(s.get()["obs"]["passwordEnc"], "dp1:already", "이미 이 판 방식인 것은 그대로");
        assert_eq!(adopt_electron_secrets(&mut s, Some(&old)), 0, "두 번 하지 않는다");
        let _ = std::fs::remove_dir_all(d);
    }
    /// 진짜 Electron safeStorage 로 잠근 값을 푸는지: WP_ESEC=<base64> WP_ELS=<Local State 경로> WP_EPLAIN=<기대 평문>
    #[test]
    #[ignore]
    fn real_electron_safestorage_blob() {
        let (Ok(enc), Ok(ls), Ok(want)) = (std::env::var("WP_ESEC"), std::env::var("WP_ELS"), std::env::var("WP_EPLAIN")) else { return };
        assert_eq!(electron_decrypt(enc.trim(), std::path::Path::new(&ls)).as_deref(), Some(want.as_str()));
        println!("Electron safeStorage 값 풀림: 일치");
    }    /// 이 PC 의 실제 1.x 설정으로 (읽기만, 값은 찍지 않는다): cargo test -p wp-core --offline real_electron_secret -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_electron_secret() {
        let old = dirs::data_dir().unwrap().join("WorshipCue").join("data");
        let s: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(old.join("settings.json")).unwrap()).unwrap();
        for (sec, key) in [("ai", "keyEnc"), ("obs", "passwordEnc")] {
            let enc = s[sec][key].as_str().unwrap_or("");
            let r = electron_decrypt(enc, &old.parent().unwrap().join("Local State"));
            println!("{sec}.{key}: 저장된 값 {}글자 → 풀림 {} (길이 {})", enc.len(), r.is_some(), r.map(|x| x.chars().count()).unwrap_or(0));
            for d in std::fs::read_dir(dirs::data_dir().unwrap()).unwrap().flatten() {
                let ls = d.path().join("Local State");
                if ls.is_file() && electron_decrypt(enc, &ls).is_some() {
                    println!("  → 이 열쇠로 풀림: {}", ls.display());
                }
            }            // 어느 단계에서 막히는지 (값은 찍지 않는다)
            if !enc.is_empty() {
                let data = unb64(enc).unwrap_or_default();
                let ls: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(old.parent().unwrap().join("Local State")).unwrap_or_default()).unwrap_or_default();
                let ek = unb64(ls["os_crypt"]["encrypted_key"].as_str().unwrap_or("")).unwrap_or_default();
                let key = ek.strip_prefix(b"DPAPI").and_then(imp::unprotect);
                println!("  머리 {:?} · 전체 {}바이트 · Local State 열쇠 {}바이트(DPAPI 머리 {}) · 열쇠 풀림 {:?}", String::from_utf8_lossy(&data[..3.min(data.len())]), data.len(), ek.len(), ek.starts_with(b"DPAPI"), key.as_ref().map(|k| k.len()));
            }
        }
    }
}
