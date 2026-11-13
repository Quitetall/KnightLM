//! **blut-knightlm**: the KnightLM cookbook for BLUT.
//!
//! One recipe, `knightlm_lora`, takes the audited knightlm-lichess-v2 corpus to
//! a registered, evaluated image-text LoRA adapter. It adds `materialize_corpus`,
//! `evaluate_checkpoint`, the run-state `claim_run` and `register_run`, and a
//! rerunnable `register_corpus` around the standard `register_dataset`; the
//! trainer is the standard `hf_sft_train` (blut-cookbooks, image-text support).
//! The Python payload (`python/knightlm_blut`) holds the corpus conversion and
//! the evaluation wrapper; the prompt itself is always `model.prompt`.

pub mod profile;
pub mod recipes;
pub mod runctx;
pub mod stages;

pub use blut::framework::{Cookbook, Registry};

use blut::recipes::recipe::RecipeDef;

pub static KNIGHTLM_RECIPES: &[&RecipeDef] = &[&recipes::knightlm_lora::DEF];

/// The KnightLM cookbook: the recipes plus the stages they wire.
pub struct KnightlmCookbook;

impl Cookbook for KnightlmCookbook {
    fn name(&self) -> &'static str {
        "knightlm"
    }
    fn recipes(&self) -> &'static [&'static RecipeDef] {
        KNIGHTLM_RECIPES
    }
}

/// A [`Registry`] containing exactly the KnightLM cookbook.
pub fn registry() -> Registry {
    let mut r = Registry::new();
    r.register(Box::new(KnightlmCookbook));
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookbook_registers_its_recipe() {
        let r = registry();
        assert!(r.find("knightlm_lora").is_some());
        assert_eq!(r.all().count(), 1);
        assert_eq!(KnightlmCookbook.name(), "knightlm");
    }
}
