fn bounded_count(value: u64) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::bounded_count;

    #[test]
    fn zero() {
        let expected: f64 = 0.0;
        assert_eq!(bounded_count(0_u64).to_bits(), expected.to_bits(), "input=0; reporting should preserve this exactly representable integer");
    }

    #[test]
    fn one() {
        let expected: f64 = 1.0;
        assert_eq!(bounded_count(1_u64).to_bits(), expected.to_bits(), "input=1; reporting should preserve this exactly representable integer");
    }

    #[test]
    fn u32_max() {
        let expected: f64 = 4294967295.0;
        assert_eq!(bounded_count(4294967295_u64).to_bits(), expected.to_bits(), "input=4294967295; reporting should preserve this exactly representable integer");
    }

    #[test]
    fn four_gib() {
        let expected: f64 = 4294967296.0;
        assert_eq!(bounded_count(4294967296_u64).to_bits(), expected.to_bits(), "input=4294967296; reporting should preserve this exactly representable integer");
    }

    #[test]
    fn four_gib_plus_one() {
        let expected: f64 = 4294967297.0;
        assert_eq!(bounded_count(4294967297_u64).to_bits(), expected.to_bits(), "input=4294967297; reporting should preserve this exactly representable integer");
    }

    #[test]
    fn ten_gib() {
        let expected: f64 = 10737418240.0;
        assert_eq!(bounded_count(10737418240_u64).to_bits(), expected.to_bits(), "input=10737418240; reporting should preserve this exactly representable integer");
    }

    #[test]
    fn f64_exact_integer_limit_minus_one() {
        let expected: f64 = 9007199254740991.0;
        assert_eq!(bounded_count(9007199254740991_u64).to_bits(), expected.to_bits(), "input=9007199254740991; reporting should preserve this exactly representable integer");
    }

    #[test]
    fn f64_exact_integer_limit() {
        let expected: f64 = 9007199254740992.0;
        assert_eq!(bounded_count(9007199254740992_u64).to_bits(), expected.to_bits(), "input=9007199254740992; reporting should preserve this exactly representable integer");
    }

}
