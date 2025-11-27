# Style guide

We require the official Rust formatter and clippy linters. In addition to that, please also consider the following best-effort aspects:

- Avoid [magic numbers](https://en.wikipedia.org/wiki/Magic_number_(programming)) and strings. Instead, add them as module constants.
- Avoid abbreviated variable and function names. Always provide meaningful and readable symbols.
- Don't write macros and don't use thrid party macros for things that can easily be expressed in few lines of code or outlined into functions.
- Avoid import aliasing. Please use the parent or fully qualified path for conflicting symbols.
- Any inline comments must provide additional semantic meaning, explain counter-intuitive behavior or highlight non-obvious design decisions. In other words, try to make the code expressive enough to a degree it doesn't need comments expressing the same thing again in the English language. Delete such comments if your AI assistant generated them.
- Public items must have a meaningful doc comment.
- Provide a meaningful panic messages to `.expect()` or just use `.unwrap()`.

