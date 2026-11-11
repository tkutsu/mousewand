#!/usr/bin/env bash
# Starts an X11 session (xfwm4 with its compositor) and two xterms:
# class "XTerm" on the left, "Other" on the right.
bash ~/switch.sh x11 > /dev/null
sleep 3
export DISPLAY=:0
(setsid xterm -geometry 98x58+0+0 > /dev/null 2>&1 &)
sleep 1
(setsid xterm -class Other -geometry 98x58+800+0 > /dev/null 2>&1 &)
sleep 2
