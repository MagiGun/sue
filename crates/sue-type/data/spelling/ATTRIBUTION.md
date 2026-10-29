# Spelling data

| File | Author | Source | License |
| --- | --- | --- | --- |
| `en-us.dic` | Grady Ward (Moby Hyphenator II word list), converted by the DesignCraft and WordCraft contributors | Project Gutenberg eBook 3204 (`mhyph.txt`), by way of WordCraft's `assets/hyphenation/en-us.dic` | Public domain |
| `en-extra.txt` | WordCraft contributors | WordCraft's `assets/spelling/en-extra.txt` | MIT OR Apache-2.0 |

`en-us.dic` is deflated: a header line `DCHYPH1`, then one word a line,
sorted, front-coded against the line before, with a `-` at each place the
word may break. Sue's spelling (`sue_type::spell`) reads the words; the
breaks are there for hyphenation. The reading code and the English
inflection rules are adapted from WordCraft's `crates/proof` (MIT OR
Apache-2.0, <https://github.com/storytold/wordcraft>).
