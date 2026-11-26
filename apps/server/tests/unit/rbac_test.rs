//! The permission catalog and how assignments combine into access.

use rustrak::models::rbac::{
    self as rbac, Access, Action, Assignment, Grants, ProjectScope, Resource, Scope, TokenScope,
};

fn grants(pairs: &[(Resource, Action)]) -> Grants {
    Grants::of(pairs.iter().copied())
}

fn instance(g: Grants) -> Assignment {
    Assignment {
        project_id: None,
        grants: g,
    }
}

fn on_project(project_id: i32, g: Grants) -> Assignment {
    Assignment {
        project_id: Some(project_id),
        grants: g,
    }
}

#[test]
fn every_resource_key_parses_back_to_itself_and_is_unique() {
    let mut seen = std::collections::HashSet::new();
    for r in Resource::ALL {
        assert_eq!(Resource::parse(r.key()), Some(*r), "{}", r.key());
        assert!(seen.insert(r.key()), "duplicate key {}", r.key());
        assert!(!r.actions().is_empty(), "{} grants nothing", r.key());
    }
    assert_eq!(Resource::parse("nope"), None);
}

#[test]
fn settings_are_split_per_area_at_instance_scope() {
    for r in [
        Resource::Integrations,
        Resource::Storage,
        Resource::Mcp,
        Resource::Telemetry,
        Resource::GithubApp,
        Resource::Roles,
        Resource::Users,
    ] {
        assert_eq!(r.scope(), Scope::Instance, "{}", r.key());
    }
    assert_eq!(Resource::Issues.scope(), Scope::Project);
}

#[test]
fn grants_drop_actions_a_resource_does_not_accept() {
    let g = grants(&[
        (Resource::Projects, Action::Read),
        (Resource::Projects, Action::Create),
    ]);
    assert_eq!(g.pairs(), vec![(Resource::Projects, Action::Create)]);
}

/// A permission no endpoint enforces is a checkbox that does nothing.
#[test]
fn the_catalog_only_offers_permissions_an_endpoint_enforces() {
    for (resource, action) in [
        (Resource::ClientKeys, Action::Update),
        (Resource::SourceMaps, Action::Delete),
        (Resource::Telemetry, Action::Update),
        (Resource::Mcp, Action::Read),
    ] {
        assert!(
            !resource.accepts(action),
            "{}:{}",
            resource.key(),
            action.key()
        );
    }
}

#[test]
fn writing_a_resource_implies_reading_it() {
    let access = Access::new(vec![on_project(
        1,
        grants(&[(Resource::Monitors, Action::Update)]),
    )]);
    assert!(access.can(Resource::Monitors, Action::Read, Some(1)));
    assert!(!access.can(Resource::Monitors, Action::Read, Some(2)));
    assert!(!access.can(Resource::Monitors, Action::Delete, Some(1)));
    assert!(!access.can(Resource::Logs, Action::Read, Some(1)));
}

#[test]
fn a_token_limited_to_writing_a_resource_can_read_it() {
    let access = Access::new(vec![instance(Grants::all())]).capped(TokenScope {
        grants: grants(&[(Resource::Monitors, Action::Update)]),
        projects: None,
    });
    assert!(access.can(Resource::Monitors, Action::Read, Some(1)));
    assert!(!access.can(Resource::Logs, Action::Read, Some(1)));
}

/// A token keeps a read its owner only holds by writing, so it sees the project.
#[test]
fn a_token_limited_to_an_implied_read_sees_the_project() {
    let access = Access::new(vec![on_project(
        1,
        grants(&[(Resource::Issues, Action::Update)]),
    )])
    .capped(TokenScope {
        grants: grants(&[(Resource::Issues, Action::Read)]),
        projects: None,
    });
    assert!(access.can(Resource::Project, Action::Read, Some(1)));
    assert_eq!(access.visible_projects(), ProjectScope::Only(vec![1]));
    assert!(access.can(Resource::Issues, Action::Read, Some(1)));
    assert!(!access.can(Resource::Issues, Action::Update, Some(1)));
}

/// The other way round too: a token that writes shows what its owner reads
/// only when the owner can write it as well.
#[test]
fn a_token_for_a_write_its_owner_lacks_does_not_show_the_project() {
    let access = Access::new(vec![on_project(
        1,
        grants(&[(Resource::Issues, Action::Read)]),
    )])
    .capped(TokenScope {
        grants: grants(&[(Resource::Issues, Action::Update)]),
        projects: None,
    });
    assert!(!access.can(Resource::Issues, Action::Update, Some(1)));
}

/// `api-tokens:read` is everyone's tokens, not your own: making one must not
/// show the others'.
#[test]
fn creating_api_tokens_does_not_imply_reading_everyones() {
    let access = Access::new(vec![instance(grants(&[(
        Resource::ApiTokens,
        Action::Create,
    )]))]);
    assert!(!access.can(Resource::ApiTokens, Action::Read, None));
}

#[test]
fn nobody_has_access_by_default() {
    let access = Access::new(vec![]);
    assert!(!access.can(Resource::Issues, Action::Read, Some(1)));
    assert!(!access.can(Resource::Storage, Action::Read, None));
    assert_eq!(access.visible_projects(), ProjectScope::Only(vec![]));
}

#[test]
fn an_instance_assignment_covers_every_project() {
    let access = Access::new(vec![instance(grants(&[(
        Resource::Issues,
        Action::Update,
    )]))]);
    assert!(access.can(Resource::Issues, Action::Update, Some(7)));
    assert!(access.can(Resource::Issues, Action::Update, Some(99)));
    assert!(!access.can(Resource::Issues, Action::Delete, Some(7)));
    assert_eq!(access.visible_projects(), ProjectScope::All);
}

#[test]
fn a_project_assignment_only_covers_that_project() {
    let access = Access::new(vec![on_project(
        3,
        grants(&[(Resource::Issues, Action::Read)]),
    )]);
    assert!(access.can(Resource::Issues, Action::Read, Some(3)));
    assert!(!access.can(Resource::Issues, Action::Read, Some(4)));
    assert!(!access.can(Resource::Issues, Action::Read, None));
    assert_eq!(access.visible_projects(), ProjectScope::Only(vec![3]));
}

#[test]
fn instance_permissions_in_a_project_assignment_are_ignored() {
    let access = Access::new(vec![on_project(
        3,
        grants(&[(Resource::Storage, Action::Read)]),
    )]);
    assert!(!access.can(Resource::Storage, Action::Read, None));
    assert!(!access.can(Resource::Storage, Action::Read, Some(3)));
}

#[test]
fn any_project_permission_implies_seeing_the_project() {
    let access = Access::new(vec![on_project(
        5,
        grants(&[(Resource::Logs, Action::Read)]),
    )]);
    assert!(access.can(Resource::Project, Action::Read, Some(5)));
    assert!(!access.can(Resource::Project, Action::Update, Some(5)));
}

#[test]
fn instance_and_project_assignments_add_up() {
    let access = Access::new(vec![
        instance(grants(&[(Resource::Issues, Action::Read)])),
        on_project(2, grants(&[(Resource::Issues, Action::Delete)])),
    ]);
    assert!(access.can(Resource::Issues, Action::Delete, Some(2)));
    assert!(!access.can(Resource::Issues, Action::Delete, Some(1)));
    assert!(access.can(Resource::Issues, Action::Read, Some(1)));
}

#[test]
fn the_all_flag_grants_everything_everywhere() {
    let access = Access::new(vec![instance(Grants::all())]);
    for r in Resource::ALL {
        for a in r.actions() {
            let project = (r.scope() == Scope::Project).then_some(1);
            assert!(access.can(*r, *a, project), "{} {:?}", r.key(), a);
        }
    }
    assert!(access.is_owner());
}

#[test]
fn a_project_scoped_all_role_does_not_reach_the_instance() {
    let access = Access::new(vec![on_project(1, Grants::all())]);
    assert!(access.can(Resource::Project, Action::Delete, Some(1)));
    assert!(!access.can(Resource::Project, Action::Delete, Some(2)));
    assert!(!access.can(Resource::Users, Action::Read, None));
    assert!(!access.is_owner());
}

#[test]
fn you_can_only_hand_out_what_you_hold_where_you_hold_it() {
    let actor = Access::new(vec![on_project(
        1,
        grants(&[
            (Resource::Issues, Action::Read),
            (Resource::Issues, Action::Update),
            (Resource::ProjectMembers, Action::Update),
        ]),
    )]);
    let viewer = grants(&[(Resource::Issues, Action::Read)]);
    let deleter = grants(&[(Resource::Issues, Action::Delete)]);

    assert!(actor.covers(&viewer, Some(1)));
    assert!(!actor.covers(&viewer, Some(2)), "not on another project");
    assert!(!actor.covers(&viewer, None), "not instance-wide");
    assert!(!actor.covers(&deleter, Some(1)), "not more than they have");
    assert!(!actor.covers(&Grants::all(), Some(1)));
}

#[test]
fn an_owner_covers_anything() {
    let owner = Access::new(vec![instance(Grants::all())]);
    assert!(owner.covers(&Grants::all(), None));
    assert!(owner.covers(&grants(&[(Resource::Users, Action::Delete)]), Some(9)));
}

fn owner() -> Access {
    Access::new(vec![instance(Grants::all())])
}

#[test]
fn a_token_only_does_what_both_it_and_its_owner_allow() {
    let user = Access::new(vec![on_project(
        1,
        grants(&[
            (Resource::Issues, Action::Read),
            (Resource::Logs, Action::Read),
        ]),
    )]);
    let token = user.capped(TokenScope {
        grants: grants(&[
            (Resource::Issues, Action::Read),
            (Resource::Issues, Action::Delete),
        ]),
        projects: None,
    });
    assert!(token.can(Resource::Issues, Action::Read, Some(1)));
    assert!(
        !token.can(Resource::Issues, Action::Delete, Some(1)),
        "owner lacks it"
    );
    assert!(
        !token.can(Resource::Logs, Action::Read, Some(1)),
        "token lacks it"
    );
}

#[test]
fn a_token_bound_to_projects_sees_only_those() {
    let token = owner().capped(TokenScope {
        grants: grants(&[(Resource::SourceMaps, Action::Create)]),
        projects: Some(vec![2]),
    });
    assert!(token.can(Resource::SourceMaps, Action::Create, Some(2)));
    assert!(token.can(Resource::Project, Action::Read, Some(2)));
    assert!(!token.can(Resource::SourceMaps, Action::Create, Some(3)));
    assert!(!token.can(Resource::Storage, Action::Read, None));
    assert_eq!(token.visible_projects(), ProjectScope::Only(vec![2]));
    assert!(!token.is_owner());
}

#[test]
fn a_settings_only_token_sees_no_project() {
    let token = owner().capped(TokenScope {
        grants: grants(&[(Resource::Storage, Action::Read)]),
        projects: None,
    });
    assert!(token.can(Resource::Storage, Action::Read, None));
    assert!(!token.can(Resource::Project, Action::Read, Some(1)));
    assert_eq!(token.visible_projects(), ProjectScope::Only(vec![]));
}

#[test]
fn an_unrestricted_token_of_an_owner_is_an_owner() {
    let token = owner().capped(TokenScope {
        grants: Grants::all(),
        projects: None,
    });
    assert!(token.is_owner());
    assert_eq!(token.visible_projects(), ProjectScope::All);
}

#[test]
fn a_scoped_token_of_an_owner_reports_what_it_may_do() {
    let token = owner().capped(TokenScope {
        grants: grants(&[
            (Resource::Storage, Action::Read),
            (Resource::Logs, Action::Read),
        ]),
        projects: Some(vec![2]),
    });
    assert_eq!(
        token.instance_pairs(),
        vec![(Resource::Storage, Action::Read)]
    );
    assert_eq!(
        token.project_pairs(),
        vec![(2, vec![(Resource::Logs, Action::Read)])]
    );
}

#[test]
fn a_token_cannot_hand_out_what_it_was_not_given() {
    let token = owner().capped(TokenScope {
        grants: grants(&[
            (Resource::Users, Action::Update),
            (Resource::Issues, Action::Read),
        ]),
        projects: Some(vec![1]),
    });
    assert!(token.covers(&grants(&[(Resource::Issues, Action::Read)]), Some(1)));
    assert!(
        !token.covers(&grants(&[(Resource::Issues, Action::Read)]), None),
        "bound to project 1"
    );
    assert!(!token.covers(&grants(&[(Resource::Logs, Action::Read)]), Some(1)));
    assert!(!token.covers(&Grants::all(), None));
}

/// The projects a cross-project list reads from: every one when the grant is
/// instance-wide, otherwise only those it holds on, narrowed by a token.
#[test]
fn projects_where_a_grant_holds() {
    let everywhere = Access::new(vec![instance(grants(&[(Resource::Issues, Action::Read)]))]);
    assert_eq!(
        everywhere.projects_where(Resource::Issues, Action::Read),
        ProjectScope::All
    );

    let mixed = Access::new(vec![
        on_project(1, grants(&[(Resource::Issues, Action::Read)])),
        on_project(2, grants(&[(Resource::Logs, Action::Read)])),
        on_project(3, grants(&[(Resource::Issues, Action::Update)])),
    ]);
    assert_eq!(
        mixed.projects_where(Resource::Issues, Action::Read),
        ProjectScope::Only(vec![1, 3])
    );

    let token = everywhere.capped(TokenScope {
        grants: grants(&[(Resource::Issues, Action::Read)]),
        projects: Some(vec![5]),
    });
    assert_eq!(
        token.projects_where(Resource::Issues, Action::Read),
        ProjectScope::Only(vec![5])
    );
}

// Plugins' permissions: `<plugin>:<resource>`, declared in their
// rustrak.config.ts, granted in roles like Rustrak's own.

fn plugin(pairs: &[(&str, Action)]) -> Grants {
    Grants::of([]).with_plugins(pairs.iter().map(|(k, a)| (k.to_string(), *a)))
}

#[test]
fn a_plugin_permission_holds_where_its_role_does() {
    let access = Access::new(vec![on_project(
        7,
        plugin(&[("hello:hits", Action::Delete)]),
    )]);
    assert!(access.can_plugin("hello:hits", Scope::Project, Action::Delete, Some(7)));
    // Writing implies reading, as for Rustrak's own.
    assert!(access.can_plugin("hello:hits", Scope::Project, Action::Read, Some(7)));
    assert!(!access.can_plugin("hello:hits", Scope::Project, Action::Delete, Some(8)));
    assert!(!access.can_plugin("hello:stats", Scope::Project, Action::Read, Some(7)));
}

#[test]
fn an_instance_plugin_permission_holds_wherever_its_role_is_held() {
    let access = Access::new(vec![on_project(
        7,
        plugin(&[("hello:reports", Action::Read)]),
    )]);
    assert!(access.can_plugin("hello:reports", Scope::Instance, Action::Read, None));
}

#[test]
fn an_owner_holds_every_plugin_permission() {
    let access = Access::new(vec![instance(Grants::all())]);
    assert!(access.can_plugin("anything:at-all", Scope::Project, Action::Delete, Some(1)));
}

#[test]
fn a_plugin_permission_is_handed_out_only_by_who_holds_it() {
    let access = Access::new(vec![instance(plugin(&[("hello:hits", Action::Read)]))]);
    assert!(access.covers(&plugin(&[("hello:hits", Action::Read)]), None));
    assert!(!access.covers(&plugin(&[("hello:hits", Action::Delete)]), None));
}

#[test]
fn a_plugin_key_names_a_plugin_and_one_of_its_resources() {
    assert!(rbac::is_plugin_key("hello:hits"));
    assert!(rbac::is_plugin_key(&rbac::plugin_key(
        "web-traffic",
        "visitors"
    )));
    for not in ["hits", "Hello:x", "hello:", ":hits", "h:hits"] {
        assert!(!rbac::is_plugin_key(not), "{not}");
    }
}

fn plugin_scopes(key: &str, _: Action) -> Option<Scope> {
    match key {
        "hello:hits" => Some(Scope::Project),
        "hello:admin" => Some(Scope::Instance),
        _ => None,
    }
}

fn plugin_grant(key: &str, action: Action) -> Grants {
    Grants::default().with_plugins([(key.to_string(), action)])
}

#[test]
fn an_owners_token_kept_to_a_project_plugin_permission_sees_the_project() {
    // As the extractor builds it: the token's cap first, then the scopes.
    let token = Access::new(vec![instance(Grants::all())])
        .capped(TokenScope {
            grants: plugin_grant("hello:hits", Action::Read),
            projects: None,
        })
        .with_plugin_scopes(plugin_scopes);
    assert!(token.can_plugin("hello:hits", Scope::Project, Action::Read, Some(5)));
    assert!(token.can(Resource::Project, Action::Read, Some(5)));
}

#[test]
fn a_token_keeping_only_the_read_its_owners_update_implies_sees_the_project() {
    let token = Access::new(vec![on_project(
        5,
        plugin_grant("hello:hits", Action::Update),
    )])
    .capped(TokenScope {
        grants: plugin_grant("hello:hits", Action::Read),
        projects: None,
    })
    .with_plugin_scopes(plugin_scopes);
    assert!(token.can_plugin("hello:hits", Scope::Project, Action::Read, Some(5)));
    assert!(token.can(Resource::Project, Action::Read, Some(5)));
}

#[test]
fn an_instance_plugin_permission_held_through_a_project_is_held_everywhere() {
    // Like Rustrak's workspace permissions: wherever the role is held.
    let actor = Access::new(vec![on_project(
        7,
        plugin_grant("hello:admin", Action::Read),
    )])
    .with_plugin_scopes(plugin_scopes);
    assert!(actor.can_plugin("hello:admin", Scope::Instance, Action::Read, None));
    assert!(actor.covers(&plugin_grant("hello:admin", Action::Read), None));
    assert!(actor.covers(&plugin_grant("hello:admin", Action::Read), Some(9)));
    // A project one stays on its project.
    let project = Access::new(vec![on_project(
        7,
        plugin_grant("hello:hits", Action::Read),
    )])
    .with_plugin_scopes(plugin_scopes);
    assert!(!project.covers(&plugin_grant("hello:hits", Action::Read), Some(9)));
}

#[test]
fn only_a_project_scoped_plugin_permission_shows_the_project() {
    let plugin = |key: &str| Grants::default().with_plugins([(key.to_string(), Action::Read)]);
    let scope_of = |key: &str, _: Action| match key {
        "hello:hits" => Some(Scope::Project),
        "hello:admin" => Some(Scope::Instance),
        _ => None,
    };
    let project =
        Access::new(vec![on_project(5, plugin("hello:hits"))]).with_plugin_scopes(scope_of);
    assert!(project.can(Resource::Project, Action::Read, Some(5)));
    assert_eq!(project.visible_projects(), ProjectScope::Only(vec![5]));

    // Like Rustrak's workspace permissions: held wherever, it shows nothing.
    for a in [
        instance(plugin("hello:admin")),
        on_project(5, plugin("hello:admin")),
    ] {
        let access = Access::new(vec![a]).with_plugin_scopes(scope_of);
        assert!(!access.can(Resource::Project, Action::Read, Some(5)));
    }

    // Through a token, only one that keeps it.
    let cap = |plugins: Grants| TokenScope {
        grants: plugins,
        projects: None,
    };
    let kept = Access::new(vec![on_project(5, plugin("hello:hits"))])
        .with_plugin_scopes(scope_of)
        .capped(cap(plugin("hello:hits")));
    assert!(kept.can(Resource::Project, Action::Read, Some(5)));
    let dropped = Access::new(vec![on_project(5, plugin("hello:hits"))])
        .with_plugin_scopes(scope_of)
        .capped(cap(Grants::default()));
    assert!(!dropped.can(Resource::Project, Action::Read, Some(5)));
}
