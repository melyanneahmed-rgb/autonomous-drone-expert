# `ui/` — user interface package (not implemented)

This directory is a **placeholder**. No user interface exists here yet.

## Provisionally approved stack

- TypeScript, strict mode.
- React.
- **RTL-first**: Arabic is a first-class language, not a later translation. English is
  fully supported and further languages must remain addable.
- Technical terms are shown with their original English term alongside the plain-language
  wording, so a beginner is never asked to decode jargon and an expert is never denied the
  exact term.

## Not in this batch

- No `package.json`, no React, no Vite, no test runner, no component.
- No runtime dependency of any kind.

## Rules this package must always honour

1. The interface presents facts, questions, plans, states and reports. It never builds a
   protocol frame and never decides a technical value.
2. Guided physical actions (move a stick, flip a switch, tilt the aircraft, press BOOT)
   flow through a single orchestrator, not ad-hoc screens.
3. Risk is never hidden to make a screen look friendlier.
4. Every waiting screen states what is actually happening.
