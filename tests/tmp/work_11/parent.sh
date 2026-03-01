#!/bin/bash
sleep 100 &
child_pid=$!
echo "Child PID: $child_pid"
wait $child_pid
