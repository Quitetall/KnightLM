"""KnightLM evaluation harness: per-axis scorecard, no composite score.

Stockfish is used here only after the model has committed to a move (scoring
and the game opponent), never inside the serving path (WAR-0005 engine-zero).
"""

SCORECARD_SCHEMA = "knightlm-scorecard/v1"
