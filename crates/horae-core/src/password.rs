//! 密码保护模式 (Password Protection Mode)。
//!
//! 提供基于 PBKDF2-HMAC-SHA256 的密钥派生、加盐验证与透明字段级认证加密 (AEAD)，
//! 保护 horae 数据库中的敏感文本字段（任务标题、备注、委派人、检查单、事件元数据）静态加密存储。

use std::cell::Cell;

use anyhow::{bail, Result};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use rusqlite::Connection;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub const KEY_PASSWORD_ENABLED: &str = "password_enabled";
pub const KEY_PASSWORD_SALT: &str = "password_salt";
pub const KEY_PASSWORD_HASH: &str = "password_hash";
pub const ENC_PREFIX: &str = "enc:v1:";

/// 迭代次数（平衡安全性与启动性能）
pub const PBKDF2_ITERATIONS: u32 = 50_000;

thread_local! {
    static SESSION_ENC_KEY: Cell<Option<[u8; 32]>> = Cell::new(
        std::env::var("HORAE_SESSION_KEY")
            .ok()
            .and_then(|s| hex_decode(&s))
            .and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok())
    );
}

/// 设置当前会话内存中的加密密钥。
pub fn set_session_key(key: Option<[u8; 32]>) {
    SESSION_ENC_KEY.with(|slot| slot.set(key));
}

/// 清除当前会话内存中的加密密钥（例如 TUI 启动锁屏或按 L 锁屏时）。
pub fn clear_session_key() {
    set_session_key(None);
}

/// 获取当前会话已解锁的密钥（供透明加解密使用）。
pub fn session_key() -> Option<[u8; 32]> {
    SESSION_ENC_KEY.with(|slot| slot.get())
}

/// 导出会话密钥 hex（仅用于父进程已解锁时拉起后台 `pomo daemon` 子进程）。
pub fn session_key_hex() -> Option<String> {
    session_key().map(|k| hex_encode(&k))
}

fn derive_subkey(master: &[u8; 32], context: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(master).expect("HMAC key");
    mac.update(context);
    mac.finalize().into_bytes().into()
}

/// 使用指定密钥加密单段文本（HMAC-SHA256-CTR + Encrypt-then-MAC 128-bit 认证标签）。
pub fn encrypt_with_key(key: &[u8; 32], plaintext: &str) -> String {
    if plaintext.is_empty() || plaintext.starts_with(ENC_PREFIX) {
        return plaintext.to_string();
    }
    let nonce_uuid = uuid::Uuid::new_v4();
    let nonce = nonce_uuid.as_bytes();
    let pt_bytes = plaintext.as_bytes();
    let mut ciphertext = Vec::with_capacity(pt_bytes.len());

    for (block_idx, chunk) in pt_bytes.chunks(32).enumerate() {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key");
        mac.update(nonce);
        mac.update(b"enc");
        mac.update(&(block_idx as u32).to_be_bytes());
        let stream = mac.finalize().into_bytes();
        for (&b, &k) in chunk.iter().zip(stream.iter()) {
            ciphertext.push(b ^ k);
        }
    }

    let mut tag_mac = HmacSha256::new_from_slice(key).expect("HMAC key");
    tag_mac.update(nonce);
    tag_mac.update(b"mac");
    tag_mac.update(&ciphertext);
    let full_tag = tag_mac.finalize().into_bytes();

    let mut payload = Vec::with_capacity(16 + 16 + ciphertext.len());
    payload.extend_from_slice(nonce);
    payload.extend_from_slice(&full_tag[..16]);
    payload.extend_from_slice(&ciphertext);

    format!("{}{}", ENC_PREFIX, STANDARD_NO_PAD.encode(&payload))
}

/// 使用指定密钥解密 `enc:v1:` 密文；认证失败或格式损坏时返回 `None`。
pub fn decrypt_with_key(key: &[u8; 32], value: &str) -> Option<String> {
    let b64 = value.strip_prefix(ENC_PREFIX)?;
    let payload = STANDARD_NO_PAD.decode(b64).ok()?;
    if payload.len() < 32 {
        return None;
    }
    let nonce = &payload[0..16];
    let tag = &payload[16..32];
    let ciphertext = &payload[32..];

    let mut tag_mac = HmacSha256::new_from_slice(key).expect("HMAC key");
    tag_mac.update(nonce);
    tag_mac.update(b"mac");
    tag_mac.update(ciphertext);
    let expected_full = tag_mac.finalize().into_bytes();
    if !constant_time_eq(tag, &expected_full[..16]) {
        return None;
    }

    let mut plaintext = Vec::with_capacity(ciphertext.len());
    for (block_idx, chunk) in ciphertext.chunks(32).enumerate() {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key");
        mac.update(nonce);
        mac.update(b"enc");
        mac.update(&(block_idx as u32).to_be_bytes());
        let stream = mac.finalize().into_bytes();
        for (&b, &k) in chunk.iter().zip(stream.iter()) {
            plaintext.push(b ^ k);
        }
    }
    String::from_utf8(plaintext).ok()
}

/// 若当前库已开启密码保护且会话密钥就绪，对敏感字段执行透明加密。
pub fn maybe_encrypt_field(conn: &Connection, plaintext: &str) -> String {
    if plaintext.is_empty() || plaintext == "[]" || plaintext.starts_with(ENC_PREFIX) {
        return plaintext.to_string();
    }
    if is_password_enabled(conn).unwrap_or(false) {
        if let Some(key) = session_key() {
            return encrypt_with_key(&key, plaintext);
        }
    }
    plaintext.to_string()
}

/// 对读取出的字段执行透明解密（若带 `enc:v1:` 前缀且会话密钥已解锁）。
pub fn maybe_decrypt_field(value: &str) -> String {
    if !value.starts_with(ENC_PREFIX) {
        return value.to_string();
    }
    if let Some(key) = session_key() {
        if let Some(plain) = decrypt_with_key(&key, value) {
            return plain;
        }
    }
    "🔒 [已加密]".to_string()
}

/// 检查当前数据库是否已启用密码保护模式。
pub fn is_password_enabled(conn: &Connection) -> Result<bool> {
    let val = crate::repo::settings::get(conn, KEY_PASSWORD_ENABLED)?;
    Ok(matches!(val.as_deref(), Some("1")))
}

/// 检查当前数据库是否已设置过密码（无论当前开关状态）。
pub fn has_password(conn: &Connection) -> Result<bool> {
    let hash = crate::repo::settings::get(conn, KEY_PASSWORD_HASH)?;
    Ok(hash.is_some_and(|h| !h.is_empty()))
}

/// 设置新密码并启用密码保护模式（自动将库内现有明文敏感字段加密）。
pub fn set_password(conn: &Connection, password: &str) -> Result<()> {
    if password.trim().is_empty() {
        bail!("密码不能为空");
    }

    // 若此前有旧密钥加密的数据，先用当前会话密钥解密回明文再切换新密钥
    let prev_key = session_key();

    // 生成 16 字节随机盐
    let salt_uuid = uuid::Uuid::new_v4();
    let salt_bytes = salt_uuid.as_bytes();
    let salt_hex = hex_encode(salt_bytes);

    let master_bytes = pbkdf2_sha256(password.as_bytes(), salt_bytes, PBKDF2_ITERATIONS);
    let auth_bytes = derive_subkey(&master_bytes, b"horae:auth:v1");
    let enc_key = derive_subkey(&master_bytes, b"horae:enc:v1");
    let hash_hex = hex_encode(&auth_bytes);

    crate::repo::settings::set(conn, KEY_PASSWORD_SALT, &salt_hex)?;
    crate::repo::settings::set(conn, KEY_PASSWORD_HASH, &hash_hex)?;
    crate::repo::settings::set(conn, KEY_PASSWORD_ENABLED, "1")?;

    set_session_key(Some(enc_key));
    rekey_database(conn, prev_key.as_ref(), Some(&enc_key))?;

    Ok(())
}

/// 验证密码是否正确（验证通过后自动将派生的加密密钥载入当前进程内存）。
pub fn verify_password(conn: &Connection, password: &str) -> Result<bool> {
    let salt_hex = match crate::repo::settings::get(conn, KEY_PASSWORD_SALT)? {
        Some(s) if !s.is_empty() => s,
        _ => return Ok(false),
    };
    let expected_hash_hex = match crate::repo::settings::get(conn, KEY_PASSWORD_HASH)? {
        Some(h) if !h.is_empty() => h,
        _ => return Ok(false),
    };

    let salt_bytes = match hex_decode(&salt_hex) {
        Some(b) => b,
        None => return Ok(false),
    };
    let expected_hash_bytes = match hex_decode(&expected_hash_hex) {
        Some(b) => b,
        None => return Ok(false),
    };

    let master_bytes = pbkdf2_sha256(password.as_bytes(), &salt_bytes, PBKDF2_ITERATIONS);
    let auth_bytes = derive_subkey(&master_bytes, b"horae:auth:v1");
    // 兼容旧版直接存储 master_bytes 作为 password_hash 的数据库
    let matched = constant_time_eq(&auth_bytes, &expected_hash_bytes)
        || constant_time_eq(&master_bytes, &expected_hash_bytes);
    if matched {
        let enc_key = derive_subkey(&master_bytes, b"horae:enc:v1");
        set_session_key(Some(enc_key));
        // 若库中尚存未加密的旧明文行，首次验证通过时自动补齐加密
        if is_password_enabled(conn).unwrap_or(false) {
            let _ = rekey_database(conn, Some(&enc_key), Some(&enc_key));
        }
    }
    Ok(matched)
}

/// 禁用密码保护模式（将加密字段解密回明文落盘，并清除内存密钥）。
pub fn disable_password(conn: &Connection) -> Result<()> {
    if let Some(old_key) = session_key() {
        rekey_database(conn, Some(&old_key), None)?;
    }
    crate::repo::settings::set(conn, KEY_PASSWORD_ENABLED, "0")?;
    clear_session_key();
    Ok(())
}

/// 修改密码：需提供原密码验证。原密码正确返回 Ok(true)，错误返回 Ok(false)。
pub fn change_password(conn: &Connection, old_pass: &str, new_pass: &str) -> Result<bool> {
    if !verify_password(conn, old_pass)? {
        return Ok(false);
    }
    set_password(conn, new_pass)?;
    Ok(true)
}

fn transform_value(val: &str, old_key: Option<&[u8; 32]>, new_key: Option<&[u8; 32]>) -> String {
    if val.is_empty() || val == "[]" {
        return val.to_string();
    }
    let plain = if val.starts_with(ENC_PREFIX) {
        if let Some(ok) = old_key {
            decrypt_with_key(ok, val).unwrap_or_else(|| val.to_string())
        } else {
            val.to_string()
        }
    } else {
        val.to_string()
    };
    if plain.starts_with(ENC_PREFIX) {
        return plain;
    }
    match new_key {
        Some(nk) => encrypt_with_key(nk, &plain),
        None => plain,
    }
}

/// 对 `tasks` 与 `task_events` 表中的敏感文本列统一执行重加密或解密。
pub fn rekey_database(
    conn: &Connection,
    old_key: Option<&[u8; 32]>,
    new_key: Option<&[u8; 32]>,
) -> Result<()> {
    type TaskRow = (String, String, String, Option<String>, String);
    let mut stmt = conn.prepare(
        "SELECT id, title, notes, delegated_to, checklist FROM tasks WHERE id != '__journal__'",
    )?;
    let rows: Vec<TaskRow> = stmt
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let mut ev_stmt =
        conn.prepare("SELECT id, meta FROM task_events WHERE meta IS NOT NULL AND meta != ''")?;
    let ev_rows: Vec<(i64, String)> = ev_stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(ev_stmt);

    let tx = conn.unchecked_transaction()?;
    for (id, title, notes, delegated_to, checklist) in rows {
        let new_title = transform_value(&title, old_key, new_key);
        let new_notes = transform_value(&notes, old_key, new_key);
        let new_delegated = delegated_to
            .as_deref()
            .map(|d| transform_value(d, old_key, new_key));
        let new_checklist = transform_value(&checklist, old_key, new_key);
        if new_title != title
            || new_notes != notes
            || new_delegated != delegated_to
            || new_checklist != checklist
        {
            tx.execute(
                "UPDATE tasks SET title=?1, notes=?2, delegated_to=?3, checklist=?4 WHERE id=?5",
                rusqlite::params![new_title, new_notes, new_delegated, new_checklist, id],
            )?;
        }
    }
    for (ev_id, meta) in ev_rows {
        let new_meta = transform_value(&meta, old_key, new_key);
        if new_meta != meta {
            tx.execute(
                "UPDATE task_events SET meta=?1 WHERE id=?2",
                rusqlite::params![new_meta, ev_id],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// PBKDF2-HMAC-SHA256 单块算法 (RFC 8018, dkLen = 32)。
pub fn pbkdf2_sha256(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    let base_mac = HmacSha256::new_from_slice(password).expect("HMAC accepts any key size");
    let mut mac = base_mac.clone();
    mac.update(salt);
    mac.update(&1u32.to_be_bytes());
    let mut u = mac.finalize().into_bytes();
    let mut result = u;

    for _ in 1..iterations {
        let mut mac = base_mac.clone();
        mac.update(&u);
        u = mac.finalize().into_bytes();
        for (r, byte) in result.iter_mut().zip(u.iter()) {
            *r ^= byte;
        }
    }
    result.into()
}

/// 恒定时间字节比较，防止时序侧信道攻击。
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6070_pbkdf2_sha256_vector_1() {
        // P = "password", S = "salt", c = 1, dkLen = 32
        let p = b"password";
        let s = b"salt";
        let out = pbkdf2_sha256(p, s, 1);
        let expected = "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b";
        assert_eq!(hex_encode(&out), expected);
    }

    #[test]
    fn rfc6070_pbkdf2_sha256_vector_2() {
        // P = "password", S = "salt", c = 2, dkLen = 32
        let p = b"password";
        let s = b"salt";
        let out = pbkdf2_sha256(p, s, 2);
        let expected = "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43";
        assert_eq!(hex_encode(&out), expected);
    }

    #[test]
    fn password_lifecycle() {
        let mut conn = Connection::open(":memory:").unwrap();
        crate::db::migrate::run(&mut conn).unwrap();

        // 默认未开启
        assert!(!is_password_enabled(&conn).unwrap());
        assert!(!has_password(&conn).unwrap());

        // 设置密码
        set_password(&conn, "my_secret_123").unwrap();
        assert!(is_password_enabled(&conn).unwrap());
        assert!(has_password(&conn).unwrap());

        // 验证密码
        assert!(verify_password(&conn, "my_secret_123").unwrap());
        assert!(!verify_password(&conn, "wrong_password").unwrap());
        assert!(!verify_password(&conn, "").unwrap());

        // 修改密码
        assert!(!change_password(&conn, "wrong", "new_secret").unwrap());
        assert!(change_password(&conn, "my_secret_123", "new_secret").unwrap());
        assert!(verify_password(&conn, "new_secret").unwrap());
        assert!(!verify_password(&conn, "my_secret_123").unwrap());

        // 禁用密码模式
        disable_password(&conn).unwrap();
        assert!(!is_password_enabled(&conn).unwrap());
        // 依然保留了历史设置密码
        assert!(has_password(&conn).unwrap());
    }

    #[test]
    fn field_level_encryption_at_rest_lifecycle() {
        use crate::model::task::{ChecklistItem, Status};
        use crate::repo::tasks::{self, CaptureInput};

        clear_session_key();
        let mut conn = Connection::open(":memory:").unwrap();
        crate::db::migrate::run(&mut conn).unwrap();

        // 1. 先创建一条明文任务
        let t = tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "Top secret acquisition".into(),
                notes: "Confidential term sheet".into(),
                status: Status::Waiting,
                delegated_to: Some("Alice".into()),
                checklist: vec![ChecklistItem {
                    id: "c1".into(),
                    title: "Sign NDA".into(),
                    done: false,
                }],
                ..Default::default()
            },
        )
        .unwrap();

        // 2. 开启密码保护：现有行必须被加密为 enc:v1: 密文
        set_password(&conn, "vault_pass_42").unwrap();
        let (raw_title, raw_notes, raw_delegated, raw_cl): (
            String,
            String,
            Option<String>,
            String,
        ) = conn
            .query_row(
                "SELECT title, notes, delegated_to, checklist FROM tasks WHERE id = ?1",
                [&t.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert!(raw_title.starts_with(ENC_PREFIX), "title encrypted at rest");
        assert!(!raw_title.contains("Top secret"));
        assert!(raw_notes.starts_with(ENC_PREFIX), "notes encrypted at rest");
        assert!(
            raw_delegated.as_deref().unwrap().starts_with(ENC_PREFIX),
            "delegated_to encrypted at rest"
        );
        assert!(
            raw_cl.starts_with(ENC_PREFIX),
            "checklist encrypted at rest"
        );

        // 3. 会话已解锁时，通过 repo 层读取应透明解密
        let loaded = tasks::get(&conn, &t.id).unwrap();
        assert_eq!(loaded.title, "Top secret acquisition");
        assert_eq!(loaded.notes, "Confidential term sheet");
        assert_eq!(loaded.delegated_to.as_deref(), Some("Alice"));
        assert_eq!(loaded.checklist.len(), 1);
        assert_eq!(loaded.checklist[0].title, "Sign NDA");

        // 精确标题解析与模糊搜索在加密下照常工作
        assert_eq!(
            tasks::resolve_id(&conn, "Top secret acquisition").unwrap(),
            t.id
        );
        let searched = tasks::list(
            &conn,
            &tasks::ListFilter {
                status: None,
                tags: vec![],
                query: Some("acquisition".into()),
                review_stale: false,
            },
        )
        .unwrap();
        assert_eq!(searched.len(), 1);

        // 4. 清除会话密钥（模拟锁屏/未认证读取）：返回掩码，绝不泄露明文
        clear_session_key();
        let locked_view = tasks::get(&conn, &t.id).unwrap();
        assert_eq!(locked_view.title, "🔒 [已加密]");
        assert_eq!(locked_view.notes, "🔒 [已加密]");

        // 5. 重新验证解锁并修改密码（重加密）
        assert!(verify_password(&conn, "vault_pass_42").unwrap());
        assert!(change_password(&conn, "vault_pass_42", "new_vault_pass_99").unwrap());
        let unlocked = tasks::get(&conn, &t.id).unwrap();
        assert_eq!(unlocked.title, "Top secret acquisition");

        // 6. 禁用密码保护：落盘数据恢复为明文
        disable_password(&conn).unwrap();
        let (plain_title, plain_notes): (String, String) = conn
            .query_row(
                "SELECT title, notes FROM tasks WHERE id = ?1",
                [&t.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(plain_title, "Top secret acquisition");
        assert_eq!(plain_notes, "Confidential term sheet");
    }
}
