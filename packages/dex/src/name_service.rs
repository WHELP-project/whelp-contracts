// Validate the name validity
#[macro_export]
macro_rules! validate_name {
    ($($name:expr),*) => {
        {
            $(
                assert!($name.ends_with(".whelp"), "invalid registry provided");

                assert!((9..=18).contains(&$name.len()), "invalid name length. Min 3 - Max 12");
            )*
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_name_registry() {
        let domain = "awesome.whelp";
        validate_name!(domain);
    }

    #[test]
    #[should_panic(expected = "invalid registry provided")]
    fn test_invalid_name_registry() {
        let domain = "awesome.com";
        validate_name!(domain);
    }

    #[test]
    #[should_panic(expected = "invalid name length. Min 3 - Max 12")]
    fn test_invalid_name_len() {
        let domain = "iwanteveryonetoknowireadshakespeare.whelp";
        validate_name!(domain);
    }
}
