## rules for project **KnightByte**

## goal of the project: Full Migration from the predecessor C# engine to rust Knightbyte

Fully optimized the predecessor engine code from the predecessor tree has to be migrated and optimized to become a rust program. The Neural Network used keeps unchanged.

## when adding or changing any code it is mandatory to add inline code documentation

this should make it easier for contributers later to add to the project.

## Never add any references to files which will not be published with the code.

## Never ever add names of other engines, used blueprints, authors, AI assistants or other companies to the code. Add generic code documentation and there is no need to mention the use of references at all

We will later give credits elsewhere.

## reference material stays local

This is a pure migration project. Any private reference material used during the
migration is kept outside the repository and is never referenced by code, tests
or published documentation.

## try to keep the .rs files a small as possible and put always related code in the same file.

It is better to have more .rs files than to have a few very large .rs files. If a file becomes to large it is better to split it into multiple files to keep the code maintainable

## Try to reuse (migrate) unit tests from the predecessor if that makes sense; otherwise write new ones for Rust

## if you find a new bug also report it to **/docs-new/new-bugs.md**

## always keep **KnightByte-feature-list.md** up-to-date

## always keep **migration-plan.md** up-to-date

after each step is finished first update the documentaion before the next step.

## RULE HARDENING: test FENs must be Stockfish-verified AT TEST TIME

Every FEN constant used in a test must be verified against Stockfish first. On macOS stockfish is installed here: /opt/homebrew/bin/stockfish

## the code must compile with no errors and warnings
