- **The provider catalogue records where a model's limits were read and when
  its provider retires it.** `nika/llm-providers@1.2` lets a model row carry
  `limits_source` with `as_of`, and a `lifecycle` (`retires_not_before`,
  `successor`, `source`, `as_of`) whose successor is another model of the same
  provider; `nika_catalog::model_facts` returns them. Two nicknames may name
  one model only as aliases with identical limits and facts. Rows without
  these facts keep their exact shape.
