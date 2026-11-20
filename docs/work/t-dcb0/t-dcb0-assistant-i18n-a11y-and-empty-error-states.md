---
format: openwarrant/1.0
profile: ticket
alias: t-dcb0
uuid: 01a1a54a-3167-7318-98b7-421d827d0bbf
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Translate and label the new surfaces; cover setup/no-session/error states."
---
# Assistant i18n, a11y and empty/error states

## Checklist

- [x] English/Spanish strings for the Assistant (i-2295, after t-2883, t-f436, t-6096, t-e4fa, t-e777, t-230b, t-4bfb) — done by claude, 2026-11-06 [observed]: The Assistant table is complete in both languages, and every_key_the_views_ask_for_exists scans the view sources for i18n::t literals and fails on any key the table lacks (it found settings.multipv, blank in Settings). Nineteen duplicate rows removed - t() takes the first match, so the later copies were dead.
- [x] ARIA labels, live regions, focus rings, skip link (i-69f7) — done by claude, 2026-11-06 [observed]: The slash palette and @-mentions are a combobox over a listbox with aria-expanded, aria-controls, aria-autocomplete, role=option and aria-selected; the composer is named; the error box is role=alert/aria-live=assertive; the sending line is a polite status; skip link, main landmark and a global :focus-visible ring are asserted by the suite rather than assumed.
- [x] No-provider, no-session, model-error, context-too-large, locked-board states (i-a8ab) — done by claude, 2026-11-06 [observed]: A banner when no provider is configured; failures map to what to do about them - a prompt past the model window, a tool that refused because the board is locked - and are left alone otherwise. No session and empty transcript already had their states.
- [x] CDP: i18n/a11y (i-5bde) — done by claude, 2026-11-06 [observed]: cdp_ai_i18n_a11y.js asserts landmarks, the skip link and its focus, the combobox/listbox wiring, the chip remove name, live regions, the no-provider banner, and switching the app to Spanish and finding the composer placeholder translated.

## CHK-ai-i18n-a11y · check — ai-i18n-a11y
run: cd /mnt/4tb/KnightLM/apps/rusty-brick-chess/frontend/tests/cdp && node cdp_ai_i18n_a11y.js
on: done
evaluates i-2295, i-69f7, i-a8ab, i-5bde
