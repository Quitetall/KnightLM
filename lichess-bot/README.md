# KnightLM × lichess-bot — the third harness mode

Plugs KnightLM's own UCI engine (`knightlm-uci`) into lichess-bot so the model
can play humans on lichess. This is mode 3 of the game harness (alongside the
headless `knightlm-match` driver and driving KnightLM through any desktop GUI).

## Why this works

lichess-bot drives engines through `python-chess`'s
`chess.engine.SimpleEngine.popen_uci(...)` — a standard UCI subprocess. Because
`knightlm-uci` speaks UCI, it drops straight in with no adapter. We verified the
full chain offline (no Lichess account needed):

- python-chess spawns `knightlm-uci`, completes the UCI handshake,
- plays 40 plies with zero illegal moves,
- lichess-bot's own `load_config` accepts `config.knightlm.yml`.

## Human setup (you do these once)

```bash
# 1. Build the engine
./setup.sh              # debug; or ./setup.sh release

# 2. Get a Lichess OAuth token
#    https://lichess.org/account/oauth/token
#    Enable: "Play the bot" + "Challenge anyone".
#    Paste it into config.knightlm.yml (token: "...").

# 3. Install lichess-bot (a git install, not on PyPI)
git clone https://github.com/lichess-bot-devs/lichess-bot.git
cd lichess-bot
pip install -r requirements.txt   # add --break-system-packages if PEP 668 blocks

# 4. Run it
python lichess-bot.py /home/brianklam/Projects/KnightLM/code/lichess-bot/config.knightlm.yml
```

The token stays in `config.knightlm.yml`, which is gitignored-adjacent (human-
written, never touched by the engine build). `setup.sh` rebuilds the binary and
re-points `engine.dir` at it; it never touches the token.

## Files

- `config.knightlm.yml` — the wired config (engine = `knightlm-uci`, protocol `uci`).
- `setup.sh` — build + re-point `engine.dir`; leaves the token alone.

## No auto-download

Stockfish, models, and lichess-bot itself are all fetched by a human, per the
repo's no-auto-download policy. This config only references binaries already on
disk.
