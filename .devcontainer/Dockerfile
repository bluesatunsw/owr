FROM docker.io/osrf/ros:lyrical-desktop-full as dev
ARG USERNAME=dcuser
ARG UID=1010
ARG GID=1010

# Create the user
RUN groupadd --gid $GID $USERNAME
RUN useradd --uid $UID --gid $GID -m $USERNAME
RUN apt-get update
RUN apt-get install -y sudo
RUN echo $USERNAME ALL=\(root\) NOPASSWD:ALL > /etc/sudoers.d/$USERNAME
RUN chmod 0440 /etc/sudoers.d/$USERNAME
RUN apt-get install -y python3-pip
RUN apt-get update && apt-get upgrade -y

# ********************************************************
# *                 ROS PAGKAGES                         *
# ********************************************************

# Install additional ros packages
RUN apt-get install -y ros-lyrical-xacro \ 
    ros-lyrical-usb-cam \
    python3-pydantic # usb-cam dependency

# Ros2Control
RUN apt-get install -y ros-lyrical-gz-ros2-control ros-lyrical-joint-state-broadcaster \
    ros-lyrical-diff-drive-controller

# Lichtblick data viewer
RUN apt-get install -y xdg-utils ros-lyrical-foxglove-bridge
RUN curl "https://github.com/lichtblick-suite/lichtblick/releases/latest/download/$(curl https://github.com/lichtblick-suite/lichtblick/releases/latest/download/latest-linux.yml -qL 2>/dev/null| grep url | cut -d ':' -f 2 | tr -d ' ')" -Lo lichtblick.deb \ 
    && dpkg -i lichtblick.deb \
    && rm lichtblick.deb

# Python package manager 
COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /bin/

# dependency of swerve_drive_controller
RUN apt-get install ros-lyrical-ros2-control-test-assets 

RUN echo '* soft rtprio 99\n\
* soft priority 99\n\
* soft memlock unlimited\n\
* hard rtprio 99\n\
* hard priority 99\n\
* hard memlock unlimited' >> /etc/security/limits.conf
RUN apt-get install -y fish neovim

# ********************************************************
# *                END ROS PAGKAGES                      *
# ********************************************************
# TODO? ENV ROS_DOMAIN_ID $ROS_DOMAIN_ID

# Set up user env, password is an empty string (encrypted)
RUN chsh -s $(which bash) $USERNAME
RUN echo $USERNAME:U6aMy0wojraho | chpasswd -e
ENV SHELL /bin/bash

# Set the default user.
USER $USERNAME
CMD ["/bin/bash"]
