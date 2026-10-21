# RaTeX

Breeze typesets display formulas with RaTeX (`ratex-gpui`, `ratex-layout`,
`ratex-parser`, `ratex-render`, and their sibling crates), resolved in
`Cargo.lock` at 0.1.15 / `ratex-gpui` 0.4.1.

Copyright (c) 2026 Will Lehnertz and the RaTeX contributors.

RaTeX is distributed under the MIT License:

```
MIT License

Copyright (c) 2026 Will Lehnertz

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

The `ratex-render` build used by Breeze embeds the KaTeX fonts, which are
distributed under the SIL Open Font License 1.1. Their provenance and full
license are carried in `NOTICE-KATEX-FONTS.txt` and
`LICENSE-KATEX-FONTS-OFL.txt` in this directory.
