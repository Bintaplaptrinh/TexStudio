# Third-party notices

PDF.js 6.4.299 is bundled in `assets/pdfjs` from the Mozilla `pdfjs-dist` npm
package. Its Apache-2.0 license is included at `assets/pdfjs/LICENSE`. The
upstream JavaScript copyright notices are preserved in both module files.
Source: https://github.com/mozilla/pdf.js

Rust dependencies and exact versions are recorded in Cargo.lock. Their
individual licenses continue to apply. MiKTeX and the operating system WebView
runtime are installed separately and are not included in the executable.

LaTeX Workshop's 52 templates from `data/latex-snippet.json` are bundled at
`assets/snippets/latex-workshop.json`. Copyright (c) 2016 James Yu, MIT license;
the complete license is included in `assets/snippets/LaTeX-Workshop-LICENSE.txt`.
Source: https://github.com/James-Yu/LaTeX-Workshop

Its macro and environment catalogs and normalized package/class name catalogs
are also bundled under `assets/completions/`, under the same MIT license.

Aerijo/autocomplete-latex's `lib/resources/builtinCompletions.json` is bundled
at `assets/completions/aerijo.json`. Copyright (c) 2017 Aerijo, MIT license;
the complete license is included at `assets/completions/Aerijo-LICENSE.txt`.
Upstream revision: `54fd905133f2c87c2ffc872fc3169995b587bf96`.
Source: https://github.com/Aerijo/autocomplete-latex
The Atom provider runtime and its dependencies are not included.

HyperSnips rules are loaded from the user's local `.hsnips` file at runtime.
The supplied local reference is https://github.com/yfzhao20/Hsnips-Snippets,
`for 0.2.7+/latex.hsnips`. This repository does not include a license granting
redistribution, so its rules and global script are not bundled in this app.

Overleaf and Rust/UI remain design and architecture references. Overleaf's AGPL
source has not been incorporated into this Apache-2.0 project. Any future code
reuse must retain upstream notices and use compatible terms.

The original `assets/logo.png` supplied in this repository is preserved.
`logo-mark.png` and `logo.ico` are resized monochrome derivatives for UI and
native app icons.
