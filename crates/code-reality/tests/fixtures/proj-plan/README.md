# proj_* fixture provenance

These fixtures are GENERATED — regenerate after touching the corpus or
the minter:

```
# real leg (temp corpus = pyproject `proj-fixture 0.1.0` + fixmod/{__init__,calc,real_caller,planned_coordinator}.py
#            — mirror of tests/overlay_gen.rs `corpus()` plus the planned file)
pyrefly-index --repo <corpus> --out ../proj_real_leg.scip
overlay-gen --plan plan.toml --sources sources --out ../proj_overlay_leg.scip --report ../proj_overlay_report.toml
```

`calc.py` carries `compute` (called by real_caller + planned) and
`untouched_helper` (DEF, zero callers — the HOLE-demo claim target).
