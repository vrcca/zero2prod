use validator::ValidateEmail;

#[derive(Debug)]
pub struct SubscriberEmail(String);

impl SubscriberEmail {
    fn parse(value: String) -> Result<SubscriberEmail, String> {
        if value.validate_email() {
            Ok(Self(value))
        } else {
            Err(format!("{} is not a valid subscriber email.", value))
        }
    }
}

impl AsRef<str> for SubscriberEmail {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use claims::{assert_err, assert_ok};

    use crate::domain::SubscriberEmail;

    #[test]
    fn empty_string_is_rejected() {
        let email = "".to_string();
        assert_err!(SubscriberEmail::parse(email));
    }

    #[test]
    fn email_missing_at_symbol_is_rejected() {
        let email = "somemissing.com".to_string();
        assert_err!(SubscriberEmail::parse(email));
    }

    #[test]
    fn email_missing_subject_is_rejected() {
        let email = "@somemissing.com".to_string();
        assert_err!(SubscriberEmail::parse(email));
    }

    #[test]
    fn email_correct_email_is_valid() {
        let email = "not@somemissing.com".to_string();
        assert_ok!(SubscriberEmail::parse(email));
    }
}
