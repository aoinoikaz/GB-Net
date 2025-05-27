#[cfg(test)]
mod tests {
    use gbnet_macros::NetworkSerialize;
    
    #[derive(NetworkSerialize)]
    struct TestStruct {
        field: u8,
    }
    
    #[test]
    fn test_macro_works() {
        let _ = TestStruct { field: 42 };
    }
}