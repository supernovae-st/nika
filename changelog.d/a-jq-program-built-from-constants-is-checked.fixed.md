- **A jq step built from a `const:` value is compiled by `nika check`.** A `nika:jq` expression
  holding `${{ const.NAME }}` was skipped as « built at run time », so a constant set unquoted in
  the program (`${{ const.target_dir }} + "/"`, read by jq as names) checked clean and failed the
  first paid run with « undefined filter or variable ». The check now builds such a program
  exactly as the run does and compiles it; a template only the run can know is still left to the
  run.
