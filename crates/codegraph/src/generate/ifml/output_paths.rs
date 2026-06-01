use std::path::PathBuf;

type RoutePageFn = Box<dyn Fn(&str) -> PathBuf + Send + Sync>;
type RouteLoadFn = Box<dyn Fn(&str) -> PathBuf + Send + Sync>;
type NavMapFn = Box<dyn Fn() -> PathBuf + Send + Sync>;
type HelpersFn = Box<dyn Fn() -> PathBuf + Send + Sync>;

pub struct OutputPaths {
    pub route_page: RoutePageFn,
    pub route_load: Option<RouteLoadFn>,
    pub navigation_map: NavMapFn,
    pub route_helpers: Option<HelpersFn>,
}

impl OutputPaths {
    pub fn for_framework(framework: &str) -> Self {
        match framework {
            "svelte" => Self {
                route_page: Box::new(|name| PathBuf::from(format!("src/routes/{}/+page.svelte", name.to_lowercase()))),
                route_load: Some(Box::new(|name| PathBuf::from(format!("src/routes/{}/+page.ts", name.to_lowercase())))),
                navigation_map: Box::new(|| PathBuf::from("src/lib/routes.ts")),
                route_helpers: Some(Box::new(|| PathBuf::from("src/lib/route-helpers.ts"))),
            },
            "react" => Self {
                route_page: Box::new(|name| PathBuf::from(format!("app/{}/page.tsx", codegraph_naming::to_kebab_case(name)))),
                route_load: Some(Box::new(|name| PathBuf::from(format!("app/{}/page.server.ts", codegraph_naming::to_kebab_case(name))))),
                navigation_map: Box::new(|| PathBuf::from("src/lib/routes.ts")),
                route_helpers: None,
            },
            "vue" => Self {
                route_page: Box::new(|name| PathBuf::from(format!("pages/{}.vue", codegraph_naming::to_kebab_case(name)))),
                route_load: None,
                navigation_map: Box::new(|| PathBuf::from("src/lib/routes.ts")),
                route_helpers: None,
            },
            "flutter" => Self {
                route_page: Box::new(|name| PathBuf::from(format!("lib/screens/{}_screen.dart", codegraph_naming::to_snake_case(name)))),
                route_load: None,
                navigation_map: Box::new(|| PathBuf::from("lib/app/routes.dart")),
                route_helpers: None,
            },
            "swiftui" => Self {
                route_page: Box::new(|name| PathBuf::from(format!("Views/{}View.swift", name))),
                route_load: None,
                navigation_map: Box::new(|| PathBuf::from("App/Navigation/RouteMap.swift")),
                route_helpers: None,
            },
            _ => Self {
                route_page: Box::new(|name| PathBuf::from(format!("src/routes/{}/+page.svelte", name.to_lowercase()))),
                route_load: Some(Box::new(|name| PathBuf::from(format!("src/routes/{}/+page.ts", name.to_lowercase())))),
                navigation_map: Box::new(|| PathBuf::from("src/lib/routes.ts")),
                route_helpers: Some(Box::new(|| PathBuf::from("src/lib/route-helpers.ts"))),
            },
        }
    }
}
