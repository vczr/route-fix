use std::{env};
use std::io::ErrorKind;
use std::thread::sleep;
use std::time::Duration;
use futures::StreamExt;
use net_route::{Handle, Route, RouteChange};
use local_ip_address::{list_afinet_netifas};


#[tokio::main]
async fn main() {
    println!("开始执行路由修复, 请用wifi连接互联网，网线连接办公网。 支持以下参数: \n指定网络出口 --interface=en0 默认值en0 \n指定出口网关 --gateway=192.168.124.1");
    let command_line_args = read_args();
    println!("===================================================");
    println!("设定互联网出口:{}", command_line_args.interface);
    let route_info = read_local_ip_addr(&command_line_args.interface).unwrap();
    println!("当前wifi的ip是{}", route_info.ip);
    let gateway = {
        if command_line_args.gateway == "" {
            println!("网关采用默认值{}", route_info.gateway);
            route_info.gateway
        } else {
            println!("网关采用设置值{}", command_line_args.gateway);
            command_line_args.gateway
        }
    };
    println!("===================================================");
    let default_route = Route::new("0.0.0.0".parse().unwrap(), 0)
        .with_ifindex(0)
        .with_gateway(gateway.parse().unwrap());
    let default_route_thread = default_route.clone();
    //新建一个线程定时取查看默认路由还有没有
    tokio::spawn(async move {
        println!("新建一个线程用来检测默认路由");
        let handle = Handle::new().unwrap();
        loop {
            let default_route_now = handle.default_route().await.unwrap().unwrap();
            // println!("当前默认路由：{:?}",default_routes);
            if default_route_now.destination.to_string() != "0.0.0.0"
                || default_route_now.gateway.is_none()
                || default_route_now.gateway.unwrap().to_string() != default_route.gateway.unwrap().to_string()
            {
                println!("默认路由被修改，开始修复");
                let _ = handle.add(&default_route_thread).await;
            } else {
                // println!("默认路由正常")
            }
            sleep(Duration::from_secs(1));
        }
    });
    loop {
        println!("开启路由修改事件监听");
        let handle = Handle::new().unwrap();
        let listener = handle.route_listen_stream();
        futures::pin_mut!(listener);
        let _ = handle.add(&default_route).await;
        while let Some(event) = listener.next().await {
            match event {
                RouteChange::Add(_) => {}
                RouteChange::Delete(_) => {
                    let add_res = handle.add(&default_route).await;
                    match add_res {
                        Ok(_) => {
                            println!("修复完成了");
                        }
                        Err(e) => {
                            match e.kind() {
                                ErrorKind::PermissionDenied => {
                                    println!("没有权限");
                                }
                                ErrorKind::AlreadyExists =>{
                                    //特意不处理
                                }
                                _ => {
                                    println!("添加失败，原因{}",e.kind());
                                }
                            }
                        }
                    }
                }
                RouteChange::Change(_) => {}
            };
            // println!("===================================================");
        }
        println!("意外结束");
    }


}

fn read_local_ip_addr(interface: &str) -> Option<RouteInfo> {
    let ips = list_afinet_netifas().unwrap();
    let mut route_info = RouteInfo {
        ip: "".to_string(),
        gateway: "".to_string(),
    };
    for x in ips {
        //只要IPv4
        if x.1.is_ipv4() && x.0 == interface {
            // println!("{} {}", x.0, x.1)
            route_info.ip = x.1.to_string();
            route_info.gateway = ip_to_gateway(&route_info.ip);
            return Option::from(route_info);
        }
    }
    Option::None
}

//根据ip计算他的默认网关地址
fn ip_to_gateway(ip: &str) -> String {
    let mut ip_part: Vec<&str> = ip.split(".").collect();
    ip_part[3] = &"1";
    {
        let mut gateway = String::from("");
        for i in 0..ip_part.len() {
            gateway.push_str(ip_part.get(i).unwrap());
            if i != ip_part.len() - 1 {
                gateway.push_str(".")
            }
        }
        gateway.clone()
    }
}

fn read_args() -> CommandLineArgs {
    let mut arg = CommandLineArgs {
        gateway: "".to_string(),
        interface: "en0".to_string(),
    };
    for x in env::args() {
        let t = read_args_from_str(&x);
        match t {
            None => {}
            Some(t) => {
                if t.key == "gateway" {
                    arg.gateway = t.value;
                } else if t.key == "interface" {
                    arg.interface = t.value;
                }
            }
        }
    }
    arg
}


fn read_args_from_str(line: &str) -> Option<KValue> {
    //用=分隔
    let values = line.split_once("=");
    match values {
        Some(t) => {
            Option::from(
                KValue {
                    key: t.0.replace("--", "").parse().unwrap(),
                    value: t.1.parse().unwrap(),
                }
            )
        }
        None => {
            None
        }
    }
}

/*
用于储存路由信息
 */
struct RouteInfo {
    ip: String,
    gateway: String,
}
#[derive(Default)]
struct CommandLineArgs {
    gateway: String,
    interface: String,
}

struct KValue {
    key: String,
    value: String,
}

