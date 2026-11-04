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

- [ ] English/Spanish strings for the Assistant (i-2295, after t-2883, t-f436, t-6096, t-e4fa, t-e777, t-230b, t-4bfb)
- [ ] ARIA labels, live regions, focus rings, skip link (i-69f7)
- [ ] No-provider, no-session, model-error, context-too-large, locked-board states (i-a8ab)
- [ ] CDP: i18n/a11y (i-5bde)

## CHK-ai-i18n-a11y · check — ai-i18n-a11y
run: cd /mnt/4tb/development/tmp/opencode && node cdp_ai_i18n_a11y.js
on: done
evaluates i-2295, i-69f7, i-a8ab, i-5bde
