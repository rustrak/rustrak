//! `inspect`: what a plugin's code declares, read from the component
//! without running it: the routes its macros wrote into `rustrak:routes`,
//! and whether it imports its database.

use rustrak::plugins::inspect::inspect;

use crate::common::plugins::hello_wasm;

#[test]
fn routes_are_read_from_every_file_of_the_code() {
    let mut routes: Vec<String> = inspect(hello_wasm())
        .unwrap()
        .routes
        .iter()
        .map(|r| {
            serde_json::to_string(r)
                .unwrap()
                .replace(['{', '}', '"'], "")
        })
        .collect();
    routes.sort();
    assert_eq!(
        routes,
        [
            "method:DELETE,path:/hits,auth:user,scope:project,requires:hits:delete",
            "method:GET,path:/config,auth:user,scope:project,requires:null",
            "method:GET,path:/count,auth:user,scope:project,requires:stats:read",
            "method:GET,path:/greet,auth:user,scope:project,requires:null",
            "method:GET,path:/hits,auth:user,scope:project,requires:hits:read",
            "method:GET,path:/nap,auth:user,scope:instance,requires:null",
            "method:GET,path:/sleep,auth:user,scope:instance,requires:null",
            "method:GET,path:/spin,auth:user,scope:instance,requires:null",
            "method:POST,path:/echo,auth:user,scope:project,requires:null",
            "method:POST,path:/hit,auth:public,scope:project,requires:null",
            "method:POST,path:/hits,auth:user,scope:project,requires:null",
        ]
    );
}

#[test]
fn its_database_is_whether_it_imports_it() {
    assert!(
        inspect(hello_wasm()).unwrap().sql,
        "hello uses its database"
    );
}

#[test]
fn something_that_is_not_a_plugin_is_refused() {
    let err = inspect(b"not wasm").unwrap_err();
    assert!(err.to_string().contains("not a Rustrak plugin"), "{err}");
}
