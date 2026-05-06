# Learning Examples

`MachinistLearning.axi` demonstrates candidate learning and guardrail-oriented
domain ontology material. It keeps learning concepts, guardrails, examples, and
tacit knowledge in canonical `.axi`.

`machinist_learning.cq` is the question-first coverage surface for that domain.
Keep query intent there rather than embedding ad-hoc query DSL fragments inside
the ontology representation.

Example flow:

```bash
axiograph check validate examples/learning/MachinistLearning.axi
axiograph authoring competency-questions \
  --axi examples/learning/MachinistLearning.axi \
  --cq examples/learning/machinist_learning.cq \
  --out build/examples/learning/machinist_learning_cq.json
```

Treat LLM- or training-derived material as evidence/review-plane input unless a
workflow explicitly promotes the resulting typed deltas.
