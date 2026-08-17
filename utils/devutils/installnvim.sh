# You'll need to execute this once per container rebuild
podman exec ros2_docker mkdir -p /home/dcuser/.config/
podman cp ~/.config/nvim/ ros2_docker:/home/dcuser/.config/

podman exec ros2_docker curl -L https://github.com/neovim/neovim/releases/download/stable/nvim-linux-x86_64.tar.gz --output /home/dcuser/nvim.tar.gz
podman exec ros2_docker tar xvf /home/dcuser/nvim.tar.gz -C /home/dcuser/ 
podman exec ros2_docker bash -c "echo \"export PATH=/home/dcuser/nvim-linux-x86_64/bin:\$PATH\" >> ~/.bashrc"
