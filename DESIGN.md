# rowdiff design notes

rowdiff is a working tool, opened when someone has two exports and a question
("what changed since yesterday?"). The page opens straight into that job: two
file slots, a key, Compare. No landing page in front of it.

## Color

| Token | Value | Job |
|---|---|---|
| ground | #faf9f6 | page background, warm off-white so long tables are easy on the eyes |
| ink | #1c1c1a | text and primary buttons |
| muted | #5f5c55 | secondary text, 6.4:1 on ground |
| added | #1a7f37 on #e7f5ea | rows only in the newer file |
| removed | #b42318 on #fbeae8 | rows only in the older file |
| changed | #8a5a00 on #fdf3dc | changed cells |

Red, green and amber only ever mean removed, added and changed. Nothing else
on the page uses them. Every colored cell also has a text cue (+, -, the old
value struck through) so color is never the only signal.

## Type

Source Sans 3 for the interface, Source Code Pro for cell values and keys.
Cells are data, so they get a monospaced face with even digit widths;
everything else reads as plain UI text.

## Shape and motion

4px radius, 1px borders, no shadows, no gradients. The only motion is the
progress bar, because it shows a state that is changing.

## Voice

Written for the analyst or engineer holding the files. Short sentences, sentence
case, numbers with thousands separators. Errors say what was wrong with which
file and what to do. Words we don't use: seamless, powerful, insights,
effortless, magic.
