# Record with: vhs demo/demo.tape   (from the repo root; uses made-up sessions, not yours)
Output demo/demo.gif

Require clodo
Require fzf
Require claude

Set Shell "bash"
Set FontSize 16
Set Width 1600
Set Height 800
Set Padding 20
Set Theme "Catppuccin Mocha"
Set TypingSpeed 55ms

Hide
Type "rm -rf /tmp/clodo-demo && python3 demo/fixtures.py /tmp/clodo-demo/.claude && export CLAUDE_CONFIG_DIR=/tmp/clodo-demo/.claude PS1='$ ' && clear"
Enter
Sleep 1s
Show

Type "clodo"
Sleep 500ms
Enter
Sleep 2.5s
Down
Sleep 1.5s
Down
Sleep 1.5s
Up
Up
Sleep 500ms
Type "the time users kept getting kicked out"
Sleep 8s
Ctrl+F
Sleep 1s
Ctrl+U
Type "dns"
Sleep 3s
Escape
Sleep 1s
