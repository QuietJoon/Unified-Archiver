//! Crate-owned password newtype.
//!
//! Passwords crossing this library's public API are UTF-8 by contract
//! (AD 0042): every archive backend this crate wraps (`unrar`, `zip`,
//! `sevenz-rust2`, libarchive) expects a UTF-8 password. [`Password`]
//! makes that contract a type invariant — it can only be built from a
//! `String`/`&str`, so the stored bytes are valid UTF-8 by construction
//! and [`Password::as_str`] is infallible.
//!
//! The value is held in a [`SecStr`], which zeroes its buffer on drop
//! and masks the contents in its own `Debug`. `SecStr` is an
//! implementation detail: it never appears in a public signature or
//! field — callers only ever see [`Password`].

use secstr::SecStr;

/// A password for an encrypted archive.
///
/// UTF-8 by construction: the only constructors accept `impl
/// Into<String>` (see [`Password::new`] and the `From` impls), so the
/// wrapped bytes are always valid UTF-8. This is what lets
/// [`Password::as_str`] borrow the contents without a fallible decode
/// step — the non-UTF-8 case that the earlier `password_as_str` helper
/// had to guard against (AD 0042) is unrepresentable here.
///
/// The bytes are stored in a [`SecStr`], so they are zeroed when the
/// `Password` is dropped and are never printed by [`std::fmt::Debug`] or
/// [`std::fmt::Display`] — both redact to `***`.
///
/// # Examples
///
/// ```
/// use unified_archive::Password;
///
/// let pw = Password::new("hunter2");
/// assert_eq!(pw.as_str(), "hunter2");
/// // The contents are redacted in formatting output.
/// assert_eq!(format!("{pw}"), "***");
/// assert_eq!(format!("{pw:?}"), "Password(***)");
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct Password(SecStr);

impl Password {
    /// Build a password from any UTF-8 string source.
    ///
    /// Because the input is a `String`/`&str`, the stored bytes are
    /// valid UTF-8 by construction; there is no fallible path.
    pub fn new(password: impl Into<String>) -> Self {
        Self(SecStr::from(password.into()))
    }

    /// Borrow the password as `&str`.
    ///
    /// Infallible: UTF-8 validity is guaranteed at construction (see the
    /// type docs), so no decode can fail here. This replaces the
    /// fallible `password_as_str` accessor and its ~17 `?` call sites
    /// (AD 0042 amendment R0081 I5).
    pub fn as_str(&self) -> &str {
        // INVARIANT: `Password` is only ever constructed from a
        // `String`/`&str`, so `self.0` holds valid UTF-8. The `expect`
        // documents that invariant and can never fire through the
        // public API.
        std::str::from_utf8(self.0.unsecure()).expect("Password bytes are UTF-8 by construction")
    }
}

impl From<String> for Password {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for Password {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<&String> for Password {
    fn from(s: &String) -> Self {
        Self::new(s.clone())
    }
}

impl std::fmt::Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Password(***)")
    }
}

impl std::fmt::Display for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_and_as_str_round_trip() {
        let pw = Password::new("secret");
        assert_eq!(pw.as_str(), "secret");
    }

    #[test]
    fn new_accepts_string_and_str() {
        let from_str = Password::new("pw123");
        let from_string = Password::new(String::from("pw123"));
        assert_eq!(from_str.as_str(), "pw123");
        assert_eq!(from_string.as_str(), "pw123");
        assert_eq!(from_str, from_string);
    }

    #[test]
    fn from_impls_match_new() {
        let owned = String::from("abc");
        assert_eq!(Password::from("abc"), Password::new("abc"));
        assert_eq!(Password::from(owned.clone()), Password::new("abc"));
        assert_eq!(Password::from(&owned), Password::new("abc"));
    }

    #[test]
    fn into_str_via_map() {
        let opt: Option<Password> = Some("pw".into());
        assert_eq!(opt.as_ref().map(Password::as_str), Some("pw"));
        let none: Option<Password> = None;
        assert_eq!(none.as_ref().map(Password::as_str), None);
    }

    #[test]
    fn debug_and_display_redact() {
        let pw = Password::new("topsecret");
        assert_eq!(format!("{pw}"), "***");
        assert_eq!(format!("{pw:?}"), "Password(***)");
        // The plaintext must never appear in either representation.
        assert!(!format!("{pw:?}").contains("topsecret"));
        assert!(!format!("{pw}").contains("topsecret"));
    }

    #[test]
    fn utf8_multibyte_round_trips() {
        let pw = Password::new("p18-\u{00e9}\u{6f22}\u{5b57}");
        assert_eq!(pw.as_str(), "p18-\u{00e9}\u{6f22}\u{5b57}");
    }
}
