# saan test fixtures

`corpus/` is a deterministic, hand-written stand-in for a personal computer's files: notes,
work projects, code in several languages, recipes, travel plans, finance, school work, health
records and research papers. Every file covers one distinct, clearly identifiable topic so that
semantic search results can be judged unambiguously. Nothing is generated at test time; the
PDFs are committed as minimal uncompressed PDF 1.4 files (Helvetica Type1, WinAnsi text).

## Same-name files (different content)

| File name            | Locations                                                         |
|----------------------|-------------------------------------------------------------------|
| `README.md`          | `work/projectA/` (inventory sync service), `work/projectB/` (clinic booking app) |
| `meeting-notes.md`   | `notes/` (neighborhood association), `work/projectA/` (Q3 launch sync) |
| `todo.txt`           | `notes/` (personal errands), `work/` (sprint tasks)               |
| `utils.py`           | `code/python/` (bank export parsing), `school/cs101/` (number theory homework) |
| `packing-list.txt`   | `travel/kyoto/` (Japan, November), `travel/iceland/` (ring road)  |

## eval.json

A JSON array of `{"query": string, "expected": string}` objects. `query` is a natural-language
paraphrase of a file's content or intent (never its file name or title); `expected` is the single
correct file, as a path relative to `corpus/` with forward slashes. Queries cover Markdown, text,
code and PDF files, including every file of each same-name pair.
