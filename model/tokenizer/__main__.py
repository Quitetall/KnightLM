"""python -m model.tokenizer manifest [--out FILE]"""

import argparse
import sys

from model.tokenizer import vocab


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="cmd", required=True)
    m = sub.add_parser("manifest", help="print the ctok/1 manifest JSON")
    m.add_argument("--out")
    args = parser.parse_args(argv)
    text = vocab.manifest_json()
    if args.out:
        with open(args.out, "w") as handle:
            handle.write(text)
    else:
        sys.stdout.write(text)


if __name__ == "__main__":
    main()
