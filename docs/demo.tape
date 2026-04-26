# vhs tape file — generates docs/shh.gif
# install vhs: brew install vhs   (https://github.com/charmbracelet/vhs)
# regenerate:  vhs docs/demo.tape

Output docs/shh.gif

Set FontSize 14
Set Width 1200
Set Height 720
Set Padding 18
Set Theme "Builtin Tango Dark"
Set Shell "zsh"
Set TypingSpeed 60ms

# launch
Type "shh"
Enter
Sleep 1500ms

# live filter
Type "web"
Sleep 1200ms
Backspace 3
Sleep 600ms

# nav around
Down
Sleep 250ms
Down
Sleep 250ms
Down
Sleep 600ms

# add wizard
Ctrl+A
Sleep 1500ms

# pick existing group
Enter
Sleep 600ms

Type "edge-01"
Enter
Sleep 400ms
Type "edge1.example.com"
Enter
Sleep 400ms
Type "deploy"
Enter
Sleep 400ms
# key picker — pick first discovered key
Down
Sleep 600ms
Enter
Sleep 400ms
Type "22"
Enter
Sleep 400ms
Type "-A -o ServerAliveInterval=60"
Enter
Sleep 400ms
Type "edge, prod"
Enter
Sleep 400ms
Type "edge cache node"
Enter
Sleep 1500ms

# confirm
Enter
Sleep 1500ms

# rename group
Ctrl+R
Sleep 1000ms
Backspace 5
Type "Edge"
Enter
Sleep 1500ms

# quit
Escape
Sleep 600ms
