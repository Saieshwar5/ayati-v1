//! The phone app: where sign-in returns to and the app identities Cuckoo's
//! domain vouches for (iOS universal links and Android App Links).
use super::Reader;

#[derive(Clone, Default)]
pub struct Mobile {
    /// `AYATI_MOBILE_APP_SCHEME`, for development builds that cannot use a
    /// verified universal link: sign-in then returns to `<scheme>://auth/return`.
    /// Unset, it returns to `<origin>/auth/mobile/return`, the universal link.
    pub scheme: Option<String>,
    /// `AYATI_MOBILE_IOS_APP_IDS`: `TEAMID.bundle.id` values, comma separated.
    pub ios_app_ids: Vec<String>,
    /// `AYATI_MOBILE_ANDROID_PACKAGE` and its signing certificates'
    /// SHA-256 fingerprints, `AYATI_MOBILE_ANDROID_CERT_SHA256`.
    pub android: Option<(String, Vec<String>)>,
}

impl Mobile {
    pub(crate) fn read(env: &mut Reader) -> Self {
        let scheme = env.text("AYATI_MOBILE_APP_SCHEME");
        if scheme
            .as_deref()
            .is_some_and(|scheme| !valid_scheme(scheme))
        {
            env.problem(
                "AYATI_MOBILE_APP_SCHEME must be a lowercase URL scheme such as cuckoo, not http or https",
            );
        }
        let ios_app_ids = list(env, "AYATI_MOBILE_IOS_APP_IDS");
        if !ios_app_ids.iter().all(|id| valid_ios_app_id(id)) {
            env.problem(
                "AYATI_MOBILE_IOS_APP_IDS must be TEAMID.bundle.id values, comma separated",
            );
        }
        let package = env.text("AYATI_MOBILE_ANDROID_PACKAGE");
        let certificates = list(env, "AYATI_MOBILE_ANDROID_CERT_SHA256");
        let android = match package {
            Some(package) if !certificates.is_empty() => Some((package, certificates)),
            None if certificates.is_empty() => None,
            _ => {
                env.problem("Set both AYATI_MOBILE_ANDROID_PACKAGE and AYATI_MOBILE_ANDROID_CERT_SHA256, or neither");
                None
            }
        };
        if let Some((package, certificates)) = &android {
            if !valid_package(package) {
                env.problem("AYATI_MOBILE_ANDROID_PACKAGE must be an Android package name");
            }
            if !certificates.iter().all(|value| valid_fingerprint(value)) {
                env.problem(
                    "AYATI_MOBILE_ANDROID_CERT_SHA256 must be colon-separated SHA-256 fingerprints",
                );
            }
        }
        Self {
            scheme,
            ios_app_ids,
            android,
        }
    }
}

fn list(env: &Reader, name: &str) -> Vec<String> {
    env.text(name)
        .map(|value| {
            value
                .split(',')
                .map(|item| item.trim().to_owned())
                .filter(|item| !item.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn valid_scheme(scheme: &str) -> bool {
    scheme.len() <= 40
        && scheme.starts_with(|c: char| c.is_ascii_lowercase())
        && scheme
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '+' | '-' | '.'))
        && !matches!(scheme, "http" | "https" | "javascript" | "data" | "file")
}

fn valid_ios_app_id(id: &str) -> bool {
    id.split_once('.').is_some_and(|(team, bundle)| {
        team.len() == 10
            && team
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            && valid_package(bundle)
    })
}

fn valid_package(name: &str) -> bool {
    (1..=150).contains(&name.len())
        && name.contains('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn valid_fingerprint(value: &str) -> bool {
    let parts: Vec<&str> = value.split(':').collect();
    parts.len() == 32
        && parts
            .iter()
            .all(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobile_settings_are_validated_together() {
        let fingerprint = vec!["AB"; 32].join(":");
        let mut env = Reader::from_pairs(&[
            ("AYATI_MOBILE_APP_SCHEME", "cuckoo"),
            ("AYATI_MOBILE_IOS_APP_IDS", "ABCDE12345.app.cuckoo.mobile"),
            ("AYATI_MOBILE_ANDROID_PACKAGE", "app.cuckoo.mobile"),
            ("AYATI_MOBILE_ANDROID_CERT_SHA256", &fingerprint),
        ]);
        let mobile = Mobile::read(&mut env);
        assert!(env.finish("test").is_ok());
        assert_eq!(mobile.scheme.as_deref(), Some("cuckoo"));
        assert_eq!(mobile.ios_app_ids, ["ABCDE12345.app.cuckoo.mobile"]);
        assert_eq!(mobile.android.unwrap().1, [fingerprint]);

        for pairs in [
            [("AYATI_MOBILE_APP_SCHEME", "https")],
            [("AYATI_MOBILE_IOS_APP_IDS", "app.cuckoo.mobile")],
            [("AYATI_MOBILE_ANDROID_PACKAGE", "app.cuckoo.mobile")],
            [("AYATI_MOBILE_ANDROID_CERT_SHA256", "AB:CD")],
        ] {
            let mut env = Reader::from_pairs(&pairs);
            Mobile::read(&mut env);
            assert!(env.finish("test").is_err(), "{pairs:?}");
        }
    }
}
