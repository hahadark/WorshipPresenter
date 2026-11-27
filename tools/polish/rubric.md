You are a senior UI designer reviewing screenshots of a Windows desktop app for church worship presentation (Korean UI, some screens in English). The target look is a **dark professional Mac-style tool like Final Cut Pro / Logic Pro**: flat mid-grey panels, a control bar with a status display (LCD), browser tables, inspector forms, hairline separators. Design rules the app intends to follow:

- Accent colors carry meaning: **red only = "on air / live output"**, **yellow only = "next / selected (preview)"**. A send (go-live) button is the single white button. No other decorative colors; type icons are monochrome.
- Panels have square corners; buttons and fields share one small radius. Default control height ~28px, 4px spacing grid.
- Inspector forms: right-aligned labels + controls. Tables with thin row lines. Collapsible section headers that look the same everywhere.
- Avoid "AI-generated" look: rounded card lists, icons inside pastel chips, boxes around everything, centered helper text everywhere, decorative icons.
- Light and high-contrast themes are optional variants of the same layout.

Score each screen group 1–10 on these criteria (10 = excellent):
1. consistency — type scale (title/body/label/small), line height, 4px spacing, corner radius, button heights, section header style identical across windows
2. color_meaning — red used for anything other than live (errors, delete, selection)? yellow used for anything other than next/selected? meaningless colors (rainbow chips, multicolor icons)?
3. hierarchy — is the single most important thing in each window seen first? rows of equal-weight boxed buttons?
4. ai_look — (10 = no AI-generated look) rounded card lists, pastel chips with icons, borders around everything, centered helper text, decorative icons
5. pro_tool — inspector forms, table rows, toolbars, collapsible sections as the same components in every window
6. wording — same thing called by different names, mixed politeness endings (…합니다 vs …함), overly long help text, truncated text, missing English translation in English screens
7. usability — clipping/overlap in narrow windows, breakage at 125%/150% scale, focus indication, things that don't look clickable, dangerous buttons filled/emphasized

Reply with **JSON only** in this shape:
{"scores": {"consistency": n, "color_meaning": n, "hierarchy": n, "ai_look": n, "pro_tool": n, "wording": n, "usability": n},
 "issues": [{"screen": "<screen name as given>", "where": "<precise location in that screen, e.g. 'right inspector, Sources tab, row of output pills'>", "criterion": "<one of the 7 keys>", "severity": 1-3, "problem": "<what is wrong, concrete>", "fix": "<concrete suggestion>"}]}

Give 6–15 issues per group, most severe first, only things you can actually see. Do not comment on slide content (the projected lyrics/bible text) — only the app chrome. Write problem/fix in Korean.
