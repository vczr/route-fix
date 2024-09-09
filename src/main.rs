use std::{env};
use std::io::{ ErrorKind};
use std::thread::sleep;
use std::time::Duration;
use futures::StreamExt;
use net_route::{Handle, Route, RouteChange};
use local_ip_address::{list_afinet_netifas};
use log::{error, info, LevelFilter};
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Config, Root};
use std::fs;
use std::path::Path;
// use env_logger::Env;

#[tokio::main]
async fn main() {
    // env_logger::builder().filter_level(LevelFilter::Info).init();
    init_log();
    info!("开始执行路由修复, 请用wifi连接互联网，网线连接办公网。 支持以下参数: \n指定网络出口 --interface=en0 默认值en0 \n指定出口网关 --gateway=192.168.124.1");
    let command_line_args = read_args();
    info!("===================================================");
    info!("设定互联网出口:{}", command_line_args.interface);
    let route_info_option = read_local_ip_addr(&command_line_args.interface);
    match route_info_option {
        None => {
            panic!("接口{}找不到网络链接",command_line_args.interface)
        }
        _ => {}
    }
    let route_info = route_info_option.unwrap();
    info!("当前wifi的ip是{}", route_info.ip);
    let gateway = {
        if command_line_args.gateway == "" {
            info!("网关采用默认值{}", route_info.gateway);
            route_info.gateway
        } else {
            info!("网关采用设置值{}", command_line_args.gateway);
            command_line_args.gateway
        }
    };
    info!("===================================================");
    let default_route = Route::new("0.0.0.0".parse().unwrap(), 0)
        .with_ifindex(0)
        .with_gateway(gateway.parse().unwrap());
    let default_route_thread = default_route.clone();
    //新建一个线程定时取查看默认路由还有没有
    let default_role_watcher_handle = tokio::spawn(async move {
        info!("新建一个线程用来检测默认路由");
        let handle = Handle::new().unwrap();
        loop {
            let default_route_now = handle.default_route().await.unwrap().unwrap();
            // info!("当前默认路由：{:?}",default_routes);
            if default_route_now.destination.to_string() != "0.0.0.0"
                || default_route_now.gateway.is_none()
                || default_route_now.gateway.unwrap().to_string() != default_route.gateway.unwrap().to_string()
            {
                info!("默认路由被修改，开始修复");
                let _ = handle.add(&default_route_thread).await;
            } else {
                // info!("默认路由正常")
            }
            sleep(Duration::from_secs(1));
        }
    });
    let del_handler = tokio::spawn(async {
        loop {
            delete_log_files();
            sleep(Duration::from_secs(60));
        }
    });
    loop {
        info!("开启路由修改事件监听");
        let handle = Handle::new().unwrap();
        let listener = handle.route_listen_stream();
        futures::pin_mut!(listener);
        let _ = handle.add(&default_route).await;
        let mut loop_limit = 0 ;
        while let Some(event) = listener.next().await {
            match Some(event) {
                Some(RouteChange::Add(_)) => {}
                Some(RouteChange::Delete(_)) => {
                    let add_res = handle.add(&default_route).await;
                    match add_res {
                        Ok(_) => {
                            info!("修复完成了。{}",loop_limit);
                            loop_limit = loop_limit + 1;
                        }
                        Err(e) => {
                            match e.kind() {
                                ErrorKind::PermissionDenied => {
                                    info!("没有权限");
                                }
                                ErrorKind::AlreadyExists => {
                                    //特意不处理
                                }
                                _ => {
                                    info!("添加失败，原因{}", e.kind());
                                }
                            }
                        }
                    }
                }
                Some(RouteChange::Change(_)) => {}
                None => {
                    info!("Received None event");
                    break; // 退出循环
                }
            };
            // info!("===================================================");
            if loop_limit > command_line_args.loop_times {
                break;
            }
        }
        info!("下一次循环");
    }


}

fn init_log() {
    let stdout = ConsoleAppender::builder().build();
    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .build(Root::builder().appender("stdout").build(LevelFilter::Debug))
        .unwrap();
    log4rs::init_config(config).unwrap();

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
            // info!("{} {}", x.0, x.1)
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
        loop_times: 200,
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
                } else if t.key == "loop_times" {
                    arg.loop_times = t.value.parse().unwrap();
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
    loop_times:i32
}

struct KValue {
    key: String,
    value: String,
}



fn delete_log_files() {
    info!("删除vpn日志");
    let log_dir = Path::new("/Applications/F8iCloud/F8iCloudApp.app/Contents/Resources/log");
    
    if let Ok(entries) = fs::read_dir(log_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("log") {
                    if let Err(e) = fs::remove_file(&path) {
                        info!("Failed to delete log file {:?}: {}", path, e);
                    } else {
                        info!("Deleted log file: {:?}", path);
                    }
                }
            }
        }
    } else {
        info!("Failed to read log directory");
    }
}

