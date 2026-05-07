#[cfg(test)]
mod tests;

// Migration facade: `andromeda-proto` remains the public compatibility path
// while the contract-safe model moves below protocol wire generation.
pub use andromeda_structured_object::{
    RowCountPolicy, StructuredObjectHeader, StructuredObjectLayout,
};
