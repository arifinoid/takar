use crate::error::ApiError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Contact {
    Email(String),
    Phone(String),
}

impl Contact {
    pub fn parse(raw: &str) -> Result<Self, ApiError> {
        let value = raw.trim();

        if value.is_empty() {
            return Err(ApiError::BadRequest("contact_required"));
        }

        if value.contains('@') {
            let email = value.to_lowercase();
            let valid = email.len() >= 5
                && !email.starts_with('@')
                && !email.ends_with('@')
                && email.matches('@').count() == 1;

            return if valid {
                Ok(Contact::Email(email))
            } else {
                Err(ApiError::BadRequest("invalid_email"))
            };
        }

        let phone: String = value
            .chars()
            .filter(|character| !matches!(character, ' ' | '-' | '(' | ')' | '.'))
            .collect();

        // People type the number the way they dial it: "0812..." locally, or
        // with the country code and no plus. Store one canonical form so the
        // same person is one account, not three.
        let phone = if phone.starts_with('+') {
            phone
        } else if phone.starts_with("62") {
            format!("+{phone}")
        } else if let Some(rest) = phone.strip_prefix('0') {
            format!("+62{rest}")
        } else {
            return Err(ApiError::BadRequest("invalid_phone"));
        };

        let digits = phone.trim_start_matches('+');

        let valid = (8..=15).contains(&digits.len())
            && digits.chars().all(|character| character.is_ascii_digit());

        if valid {
            Ok(Contact::Phone(phone))
        } else {
            Err(ApiError::BadRequest("invalid_phone"))
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Contact::Email(value) | Contact::Phone(value) => value,
        }
    }

    pub fn email(&self) -> Option<&str> {
        match self {
            Contact::Email(value) => Some(value),
            Contact::Phone(_) => None,
        }
    }

    pub fn phone(&self) -> Option<&str> {
        match self {
            Contact::Phone(value) => Some(value),
            Contact::Email(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_email_case() {
        assert!(matches!(
            Contact::parse(" Owner@Example.TEST "),
            Ok(Contact::Email(email)) if email == "owner@example.test"
        ));
    }

    #[test]
    fn normalizes_phone_separators() {
        assert!(matches!(
            Contact::parse("+62 812-3456-7890"),
            Ok(Contact::Phone(phone)) if phone == "+6281234567890"
        ));
    }

    #[test]
    fn accepts_the_local_indonesian_format() {
        assert!(matches!(
            Contact::parse("081234567890"),
            Ok(Contact::Phone(phone)) if phone == "+6281234567890"
        ));
        assert!(matches!(
            Contact::parse("08 12-3456 7890"),
            Ok(Contact::Phone(phone)) if phone == "+6281234567890"
        ));
    }

    #[test]
    fn accepts_the_country_code_without_a_plus() {
        assert!(matches!(
            Contact::parse("6281234567890"),
            Ok(Contact::Phone(phone)) if phone == "+6281234567890"
        ));
    }

    #[test]
    fn rejects_invalid_contacts() {
        assert!(Contact::parse("").is_err());
        assert!(Contact::parse("not-an-email@").is_err());
        assert!(
            Contact::parse("8123456789").is_err(),
            "no plus, no 0, no 62: cannot tell which country"
        );
        assert!(Contact::parse("+12").is_err());
        assert!(Contact::parse("0812").is_err());
    }
}
