# vhs tape for `hm dev up` TUI demo.
#
# Prerequisite: an example with `@hm.deploy(...)` registrations in
# `.harmont/`. None of the current `examples/*` directories declare a
# deployment, so this tape references a `examples/dev-demo/` directory
# that should be added in a follow-up commit (e.g., a minimal nginx
# service so the demo has something to render).
#
# Run with: vhs docs/demo/dev.tape

Output docs/demo/dev.gif

Set FontSize 14
Set Width 1200
Set Height 720
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms

Type "cd examples/dev-demo"
Enter
Sleep 500ms
Type "hm dev up"
Enter
Sleep 20s
Screenshot docs/demo/dev.png
Sleep 2s
Ctrl+C
Sleep 2s
