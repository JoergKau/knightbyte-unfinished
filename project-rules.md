# rules for project **KnightByte**

## goal of the project: Full Migration from the predecessor C# (.Net11) chess engine PawnForgeCS ("C:\dev\Migration-Project\PawnForgeCS.win.final.dotnet11") to Knightbyte written in rust

Fully optimized engine code from the predecessor tree has to be migrated and optimized to become a rust program. The Neural Network used keeps unchanged.

## if during the migration phase quirks are found in the predecessor, that has to be reported in a file called ""C:\dev\Migration-Project\PawnForgeCS-Files\quirks-found.md"" first 

The migration goes on and KnightByte has to reproduce it and that will be all checked again later when the migration finished. 

## if during the migration phase bugs are found in the predecessor, we will stop the migration and decide what to do next

add all bugs to "C:\dev\Migration-Project\PawnForgeCS-Files\new-bugs.md" and those bugs have to be fixed first and it must be checked whether those bugs were already introduced into the KnightByte code.  

## KnightByte at the end should be 100% feature compliant but the rust implementation should be highly optimized in key features like "time to depth" and must use any language feature which may help here

use:  "C:\dev\knightbyte-unfinished\docs-new\maximizing_performance_nps_in_a_rust_uci_chess_engine.md" as a template hwo it could be achieved

## the code for the intel arm must be optimzed for the CCRL

The engine will apply for playing to be listed in the CCRL. "C:\dev\Migration-Project\PawnForgeCS-Files\CCRL-chess-engine.md"

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

Every FEN constant used in a test must be verified against Stockfish first. On macOS stockfish is installed here: /opt/homebrew/bin/stockfish. on windows it is "C:\dev\c#-chess-engines\stockfish\stockfish-windows-x86-64-universal.exe"

## the code must compile with no errors and warnings
