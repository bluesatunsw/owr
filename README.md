# owr
This repo includes all software necessary to operate the Test Rover Platform
that is being designed and developed in the OWR project.

## System Setup
A guide is available on the
[owr-docs](https://github.com/bluesatunsw/owr-docs/tree/main/src/guides/container_setup.md)
repository.

## Useful Command index
| Command                                                               | Effect                                                                                          |
| -------------------------------                                       | --------------------------------                                                                |
| `podman compose -f "$PWD/.devcontainer/docker-compose.yml" up -d`     | Run in the git root to build/restart/start the Docker container.                                |
| `podman compose -f "$PWD/.devcontainer/docker-compose-obc.yml" up -d` | Run in the git root to build/restart/start the Docker container **with obc specific settings!** |
| `podman exec -it --latest /bin/bash`                                  | Enter the docker container after launching it with the devcontainer.                            |
| `./utils/init.bash`                                                   | Run after `cd`'ing into `/workspaces/owr` in the container to source the ROS env and tools      |
| `colcon build --symlink-install`                                      | Runs the `colcon` meta build system, builds ALL rover packages!                                 |
| `ros2 launch rover rover.launch.py`                                   | Launch the robot state publisher. Just leave this running.                                      |
| `rqt`                                                                 | The flexible ROS2 viewer, useful for debugging topic flows and looking at raw topics.           |
| `rviz2`                                                               | Useful for visualizing TF2 / LiDAR / etc... data to see if it looks okay.                       |
| `podman rm --force --latest -t 0`                                     | Kill the podman container with the latest creation date (quickly)                               |

NOTE: `podman compose` will take a VERY long time! on uni Wi-Fi \~5min due to
the amount of packages it installs (\~5GB). There is also [a podman
bug](github.com/containers/podman/issues/16541) that invokes a postprocessing
step on the overlay filesystem, this will take **upwards of 10 minutes!** 

NOTE: `./utils/init.bash` will source the appropriate files as well as put you
in a 10-deep nested shell - running an invalid command will kill 1 level!

### Less Common Commands

| Command                                                               | Effect                                                                                    |
| -------------------------------                                       | --------------------------------                                                          |
| `ros2 launch rover rsp.launch.py`                                     | Robot State Publisher, publishes the URDF+JSP, to make it viewable in Lichtblick / Rviz2. |
| `xacro ./src/rover/description/robot.urdf.xacro > ./build/rover.urdf` | Compiles the xacro into a singular urdf file                                              |
| `ros2 run key_teleop key_teleop --ros-args -p twist_stamped_enabled:=True -r /key_vel:=/diff_drive_base_controller/cmd_vel` | Record keyboard imputs and publish them as Twist2d values, which ROS2Control is listening for!

### Useful Podman Commands
| Command                                                               | Effect                                                                                    |
| -------------------------------                                       | ---------------------------------                                                         |
| `podman images`                                                       | Lists current podman images                                                               |
| `podman rmi {ID} --force`                                             | Remove relevant image. Replace the ID with output from the above command                  |
