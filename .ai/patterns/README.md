# Provider-neutral development patterns

1. **One authority, thin adapters.** Policy/events are defined in canonical contracts.
   Provider-specific files only discover instructions or normalize native event fields.
   Every unique stub declares the harness/schema difference and links back.
2. **Evidence before assertions.** Preserve task -> base state -> action -> observation ->
   patch -> test -> review -> outcome. A chat's "passed" is not a test receipt.
3. **Durable incremental capture.** Journal during the session, not only on shutdown.
   Retry with stable identity; reject changed-content replay. Mark missing data explicitly.
4. **Instruction vs enforcement.** Instructions guide models. Admitted hooks run commands.
   Local policy gates cooperate with agents; remote branch protection enforces merges.
5. **No secret-by-default telemetry.** Metadata is allowlisted; content requires separate
   informed consent. No automated training eligibility from merely recording a chat.
6. **No hidden reasoning extraction.** Store authorized visible messages, concise decision
   rationale and observable tool evidence, not private internal reasoning.

Use existing [architecture decisions](../../docs/product-and-platform-plan.md), not new
abstractions, whenever the existing owner already fits.
