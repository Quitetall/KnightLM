"""Board perceiver: image -> 64-square piece classes -> BoardObservation.

See docs/model/perceiver.md. Separate from any LLM; needs numpy, Pillow and
torch, none of which the fast CI tier installs (tests skip without them).
"""
