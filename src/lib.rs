pub mod parser;
pub mod highlight;
pub mod runtime;
pub mod ffi;
pub mod diagnostic;
pub mod lexer;
pub mod syntax;
pub mod ffi_bridge;
pub mod backend_traits;

// pub use ffi_types::*;

uniffi::setup_scaffolding!();
// uniffi::include_scaffolding!("context");

// #[cfg(test)]
// mod tests {
//     #[test]
//     fn test_end_to_end_processing() {
//         use crate::runtime::Runtime;
//         
//         let runtime = Runtime::new().unwrap();
//         let content = r#"
//             \starttext
//             Hello World!
//             \stoptext
//         "#;
//         
//         runtime.open_document("test.tex".into(), content.into()).unwrap();
//         let ast = runtime.get_document_ast("test.tex");
//         assert!(ast.is_some());
//     }
// }
