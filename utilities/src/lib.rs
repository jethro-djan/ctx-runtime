pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
use std::path::Path;
use tempfile::tempdir;

#[macro_export]
macro_rules! assert_span {
    ($span:expr, $start:expr, $end:expr, $line:expr, $col:expr) => {
        assert_eq!($span.start, $start, "span start mismatch");
        assert_eq!($span.end, $end, "span end mismatch");
        assert_eq!($span.start_line, $line, "span line mismatch");
        assert_eq!($span.start_col, $col, "span column mismatch");
    }
}

pub fn create_test_context() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp_dir = tempdir().unwrap();
    let tex_path = temp_dir.path().join("test.tex");
    
    std::fs::write(&tex_path, r#"
        \starttext
        Test Content
        \stoptext
    "#).unwrap();
    
    (temp_dir, tex_path)
}
