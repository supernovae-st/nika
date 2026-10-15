- **`nika guard` judges every command a multi-line call carries.** Its
  lexer read a newline as a space and stopped at the first `#`, so a
  `nika run` on a second line, after a comment or after `a#b` passed as
  no opinion; so did one inside `$( … )`, backticks or `<( … )`, in
  `(…)` without spaces, in a function or `case` body, behind `timeout`,
  in `nika 2>&1 run`, and in the zsh forms `=nika` and `noglob`. Each is
  now judged. A heredoc body stays data, but its unquoted substitutions
  are judged, and nesting deeper than 64 levels is refused with a reason.
