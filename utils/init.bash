#!/bin/bash

# Source without args to set up your terminal environment
# Run `./init.bash owr_update` to run rosdep

# You have to manually run this!
function owr_update {
    rosdep update && rosdep install --from-paths src --ignore-src -y
}

if [[ $# -gt 0 ]]; then "$@"; exit; fi      # Run func named by first arg
source /ros_entrypoint.sh
