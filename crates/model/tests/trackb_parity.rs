// Track B parity skeleton (KLM-WAR-0011). All gates RED until implemented.
// Each test fails explicitly with a clear reason; none claims parity.

#[cfg(test)]
mod trackb_parity {
    #[test]
    #[ignore = "not implemented (KLM-SAS-RQ-013)"]
    fn processor_tokenizer_parity() {
        // RED: Burn tokenizer not yet wired.
        unimplemented!("not implemented: processor/tokenizer parity (RQ-013)");
    }
    #[test]
    #[ignore = "not implemented (KLM-SAS-RQ-013)"]
    fn reference_logits() {
        unimplemented!("not implemented: reference logits (RQ-013)");
    }
    #[test]
    #[ignore = "not implemented (KLM-SAS-RQ-013)"]
    fn gradients_match() {
        unimplemented!("not implemented: gradients (RQ-013)");
    }
    #[test]
    #[ignore = "not implemented (KLM-SAS-RQ-013)"]
    fn overfit_tiny_batch() {
        unimplemented!("not implemented: overfit (RQ-013)");
    }
    #[test]
    #[ignore = "not implemented (KLM-SAS-RQ-013)"]
    fn reload_checkpoint() {
        unimplemented!("not implemented: reload (RQ-013)");
    }
    #[test]
    #[ignore = "not implemented (KLM-SAS-RQ-013)"]
    fn reset_isolation() {
        unimplemented!("not implemented: reset isolation (RQ-013)");
    }
    // Was `assert!(true, ...)`, which cannot fail and so checked nothing while
    // reading as a passing test. The intent — a fabricated parity claim must not
    // pass — needs a reference to compare against, so it stays red with the rest
    // of the battery until one exists.
    #[test]
    #[ignore = "not implemented (KLM-SAS-RQ-013)"]
    fn fake_green_refused() {
        unimplemented!("refusal of fake-green needs a reference to refuse");
    }
}
