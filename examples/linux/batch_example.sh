#!/bin/bash
# This script demonstrates how to submit 10 jobs using range expansion
fbqueue sub --range 1-10 echo "Processing item {}"
