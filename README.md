# myrollup

此项目基于matterlabs/rollup项目的修改，去除了rollup中与eth相关的大部分代码，在没有eth环境的情况下（仍需eth浏览器或其插件）来运行rollup。去除了链上的deposit和exit操作，改用 service api 来实现存款和退出。

## Setup

1. 环境
    ### Rust
    ```shell
    curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
    ```
    *[rust教程](https://kaisery.github.io/trpl-zh-cn/title-page.html)*
    ### Node & yarn
    install node  
    install yarn

    ### PostgreSQL
    ubuntu instapp postgreSql
    ```shell
    # Create the file repository configuration:
    sudo sh -c 'echo "deb http://apt.postgresql.org/pub/repos/apt $(lsb_release -cs)-pgdg main" > /etc/apt/sources.list.d/pgdg.list'

    # Import the repository signing key:
    wget --quiet -O - https://www.postgresql.org/media/keys/ACCC4CF8.asc | sudo apt-key add -

    # Update the package lists:
    sudo apt-get update

    # Install the latest version of PostgreSQL.
    # If you want a specific version, use 'postgresql-12' or similar instead of 'postgresql':
    sudo apt-get -y install postgresql
    ```
    ubuntu install pgAdmin4
    ```shell
    #
    # Setup the repository
    #

    # Install the public key for the repository (if not done previously):
    curl https://www.pgadmin.org/static/packages_pgadmin_org.pub | sudo apt-key add

    # Create the repository configuration file:
    sudo sh -c 'echo "deb https://ftp.postgresql.org/pub/pgadmin/pgadmin4/apt/$(lsb_release -cs) pgadmin4 main" > /etc/apt/sources.list.d/pgadmin4.list && apt update'

    #
    # Install pgAdmin
    #

    # Install for both desktop and web modes:
    sudo apt install pgadmin4

    # Install for desktop mode only:
    sudo apt install pgadmin4-desktop

    # Install for web mode only: 
    sudo apt install pgadmin4-web 

    # Configure the webserver, if you installed pgadmin4-web:
    sudo /usr/pgadmin4/bin/setup-web.sh
    ```

    ### Diesel
    ```shell
    cargo install diesel_cli
    ```
    *[diesel教程](https://diesel.rs/guides/getting-started/)*

2. 编译
    ```shell
    cargo build
    # release
    carog build --release

    cd client && yarn
    ```

3. 运行  
   + server  
    ```shell
    # 设置postgresql
    cd storage
    export DATABASE_URL=postgres://username:password@localhost/diesel_demo
    echo $DATABASE_URL > .env
    diesel database setup
    diesel migration run

    # 启动server服务
    ./target/release/server
    ```
    
    + prover  
    ```shell
    ./target/release/prover
    ```

    + client  
    ```shell
    cd client && yarn dev
    ```
    访问 http://localhost:3000
    

