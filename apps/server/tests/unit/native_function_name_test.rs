//! Sentry's `trim_function_name` cases, generated from
//! `tests/sentry/stacktraces/test_functions.py` (getsentry/sentry 2458718b2c).

use rustrak::services::native::trim_function_name;

const CASES: &[(&str, &str, &str)] = &[
    ("function", "function", "native"),
    ("namespace::Class::function", "namespace::Class::function", "native"),
    (" const ", " const ", "native"),
    (" namespace::const ", "namespace::", "native"),
    ("namespace::function::h9de5fbebc1652d47", "namespace::function", "native"),
    ("?A0xc3a0617d::crash", "`anonymous namespace'::crash", "native"),
    ("Scaleform::GFx::AS3::IMEManager::DispatchEvent(char const *,char const *,char const *)", "Scaleform::GFx::AS3::IMEManager::DispatchEvent", "native"),
    ("static unsigned int Scaleform::GFx::AS3::IMEManager::DispatchEvent(char const *,char const *,char const *) const", "Scaleform::GFx::AS3::IMEManager::DispatchEvent", "native"),
    ("Scaleform::GFx::IME::GImeNamesManagerVista::OnActivated(unsigned long,unsigned short,_GUID const &,_GUID const &,_GUID const &,HKL__ *,unsigned long)", "Scaleform::GFx::IME::GImeNamesManagerVista::OnActivated", "native"),
    ("<actix_web::pipeline::Pipeline<S, H> as actix_web::server::handler::HttpHandlerTask>::poll_io", "actix_web::pipeline::Pipeline<T>::poll_io", "native"),
    ("+[FLFoo barBaz]", "+[FLFoo barBaz]", "native"),
    ("-[FLFoo barBaz]", "-[FLFoo barBaz]", "native"),
    ("<tokio_current_thread::scheduler::Scheduled<'a, U>>::tick", "tokio_current_thread::scheduler::Scheduled<T>::tick", "native"),
    ("tokio::runtime::current_thread::runtime::Runtime::enter::{{closure}}::{{closure}}::{{closure}}::{{closure}}", "tokio::runtime::current_thread::runtime::Runtime::enter::{{closure}}::{{closure}}::{{closure}}::{{closure}}", "native"),
    ("<std::panic::AssertUnwindSafe<F> as core::ops::function::FnOnce<()>>::call_once", "std::panic::AssertUnwindSafe<T>::call_once", "native"),
    ("struct style::gecko_bindings::sugar::ownership::Strong<style::gecko_bindings::structs::root::RawServoStyleSheetContents> geckoservo::glue::Servo_StyleSheet_Empty(style::gecko_bindings::structs::root::mozilla::css::SheetParsingMode) const", "geckoservo::glue::Servo_StyleSheet_Empty", "native"),
    ("static <NoType> std::panicking::begin_panic<str*>(struct str*, struct (str*, u32, u32) *)", "std::panicking::begin_panic<T>", "native"),
    ("(anonymous namespace)::StupidFunction<std::vector<std::string>>(int)", "(anonymous namespace)::StupidFunction<T>", "native"),
    ("static unsigned int _foo_function (uint32_t,uint32_t) const", "_foo_function", "native"),
    ("v8::internal::operator<<(std::__1::basic_ostream<char, std::__1::char_traits<char> >&, v8::internal::MaybeObjectBrief const&)", "v8::internal::operator<<", "native"),
    ("unsigned int (anonymous namespace)::operator<<(std::__1::basic_ostream<char, std::__1::char_traits<char> >&, v8::internal::MaybeObjectBrief const&)", "(anonymous namespace)::operator<<", "native"),
    ("unsigned int mynamespace::MyClass::operator()(std::__1::basic_ostream<char, std::__1::char_traits<char> >&, v8::internal::MaybeObjectBrief const&)", "mynamespace::MyClass::operator()", "native"),
    ("std::basic_ostream<char, std::char_traits<char> >& std::operator<< <std::char_traits<char> >(std::basic_ostream<char, std::char_traits<char> >&, char)", "std::operator<< <T>", "native"),
    ("<actix::contextimpl::ContextFut<A, C> as futures::future::Future>::poll::h9de5fbebc1652d47", "actix::contextimpl::ContextFut<T>::poll", "native"),
    ("<T as core::convert::Into<U>>::into", "core::convert::Into<T>::into", "native"),
    ("ThreadStartWhatever@16", "ThreadStartWhatever", "native"),
    ("@ThreadStartWhatever@16", "ThreadStartWhatever", "native"),
    ("NativeButtons_DoSomeWorkHere_m7486FA1E8A228E806BA045D26ABECC87DFD436B4", "NativeButtons_DoSomeWorkHere", "native"),
    ("Thread_Sleep_m71DE163765BF465EC4A0163F2ED4D43143094549(int, MethodInfo const*)", "Thread_Sleep", "native"),
    ("U3CAppHangCaptureU3Ed__12_MoveNext_m92EE64B7CB73ABCEE3CA67581B1F90122637F61D(U3CAppHangCaptureU3Ed__12_tF24197A44C378085E75CA07E995888BBF01EDA62*, MethodInfo const*)", "U3CAppHangCaptureU3Ed__12_MoveNext", "native"),
    ("List_1_CopyTo_m5180D7A430072A21034A0692CDD1D565ADEE2ABB_gshared(RuntimeArray*, int32_t)", "List_1_CopyTo_gshared", "native"),
    ("List_1_CopyTo_m5180D7A430072A21034A0692CDD1D565ADEE2ABB_gshared", "List_1_CopyTo_gshared", "native"),
    ("List_1_CopyTo_m5180D7A430072A21034A0692CDD1D565ADEE2ABB_fshared", "List_1_CopyTo_fshared", "native"),
    ("LinkedList_1_get_First_mFE6CE39F3C815C988217E69C202C990619E7EB87_inline", "LinkedList_1_get_First_inline", "native"),
    ("LinkedList_1_get_First_mFE6CE39F3C815C988217E69C202C990619E7EB87_gshared_inline", "LinkedList_1_get_First_gshared_inline", "native"),
    ("LinkedList_1_get_First_mFE6CE39F3C815C988217E69C202C990619E7EB87_fshared_inline", "LinkedList_1_get_First_fshared_inline", "native"),
    ("AnimationClipPlayable_Equals_mC5263BEA86C02CEDF93C5B14EAA168883E1DB5F4_AdjustorThunk", "AnimationClipPlayable_Equals_AdjustorThunk", "native"),
    ("GetRayIntersectionAllCallback_Invoke_m917AA4108EBDC724AFEF39BFD06A586B7461F497_Multicast", "GetRayIntersectionAllCallback_Invoke_Multicast", "native"),
    ("GetRayIntersectionAllCallback_Invoke_m917AA4108EBDC724AFEF39BFD06A586B7461F497_OpenStatic", "GetRayIntersectionAllCallback_Invoke_OpenStatic", "native"),
    ("unitytls_x509list_get_ref_t_Invoke_mE7C675B7847FFEF96C25AE757D34CE920AA16EC2_OpenInstance", "unitytls_x509list_get_ref_t_Invoke_OpenInstance", "native"),
    ("LocalCertificateSelectionCallback_Invoke_mED43EE6E88B8C653C7D68966F86751B70907591C_OpenGenericVirtual", "LocalCertificateSelectionCallback_Invoke_OpenGenericVirtual", "native"),
    ("ParameterizedConstructorDelegate_5_Invoke_m8ABE4DDEF0C49FF41403E4B3A4064E885DEC800E_OpenStaticInvoker", "ParameterizedConstructorDelegate_5_Invoke_OpenStaticInvoker", "native"),
    ("ParameterizedConstructorDelegate_5_Invoke_m8E54847F621BC27BAF3601B6E718F8243082CE3E_OpenVirtualInvoker", "ParameterizedConstructorDelegate_5_Invoke_OpenVirtualInvoker", "native"),
    ("OnVideoCaptureResourceCreatedCallback_Invoke_mE75C26918C0D14F264699056B8BB4B303246A7AD_OpenInterfaceInvoker", "OnVideoCaptureResourceCreatedCallback_Invoke_OpenInterfaceInvoker", "native"),
    ("AndroidJNIHelper_ConvertFromJNIArray_mAB7ACB3EE802BD8FCCF0D816108D296367FCA69D_gp_0_0_0_0", "AndroidJNIHelper_ConvertFromJNIArray_gp_0_0_0_0", "native"),
    ("AndroidJNIHelper_GetFieldID_mD25F75AF5FF5844520E9564616E3A25F6070B3EB_gp_0_1_0_2", "AndroidJNIHelper_GetFieldID_gp_0_1_0_2", "native"),
    ("Scope_Apply_m68049D2D67C5881750236D8E55E464E7A96A2DD7_RuntimeMethod_var", "Scope_Apply_m68049D2D67C5881750236D8E55E464E7A96A2DD7_RuntimeMethod_var", "native"),
    ("@objc ViewController.causeCrash(Any) -> ()", "ViewController.causeCrash", "native"),
    ("ViewController.causeCrash(Any) -> ()", "ViewController.causeCrash", "native"),
    ("@objc ViewController.causeCrash(Any, Foo -> Bar) -> SomeObject", "ViewController.causeCrash", "native"),
    ("ViewController.causeCrash(Any) -> SomeObject", "ViewController.causeCrash", "native"),
    ("main::$_0", "main::lambda", "native"),
    ("main::$_42", "main::lambda", "native"),
    ("main::{lambda(int)#1}", "main::lambda", "native"),
    ("main::{lambda()#42}", "main::lambda", "native"),
    ("lambda_7156c3ceaa11256748687ab67e3ef4cd", "lambda", "native"),
    ("<lambda_7156c3ceaa11256748687ab67e3ef4cd>::operator()", "<lambda>::operator()", "native"),
    ("trigger_crash_a(int*) [clone .constprop.0]", "trigger_crash_a", "native"),
    ("ShellCorona::screenInvariants() const [clone .cold]", "ShellCorona::screenInvariants", "native"),
    ("__gnu_cxx::__verbose_terminate_handler() [clone .cold]", "__gnu_cxx::__verbose_terminate_handler", "native"),
    ("std::__1::unique_ptr<X,std::default_delete<X> >::operator->", "std::__1::unique_ptr<T>::operator->", "native"),
    ("pthread_cond_timedwait@@GLIBC_2.3.2", "pthread_cond_timedwait", "native"),
    ("glob64@GLIBC_2.2", "glob64", "native"),
    ("static Namespace.ThrowingFunction() throws -> Namespace.ExitValue?", "Namespace.ThrowingFunction", "native"),
    ("closure #1 @Swift.MainActor () -> () in static Foo.CallFunction(args: [Swift.String]) -> ()", "closure in Foo.CallFunction", "native"),
    ("closure #1 () -> () in Bar.PostTask(() -> ()) -> ()", "closure in Bar.PostTask", "native"),
    ("closure #1 @Sendable () -> Swift.String in variable initialization expression of static Namespace.Class.var : Namespace.Parent", "closure in initializer expression of Namespace.Class.var", "native"),
    ("variable initialization expression of static Namespace.Class.var : Namespace.Parent", "initializer expression of Namespace.Class.var", "native"),
    ("closure #1 () -> () in variable initialization expression of static (extension in SpaceCreation):Namespace.Class.var : Namespace.Parent", "closure in initializer expression of Namespace.Class.var", "native"),
    ("thunk for @escaping @callee_guaranteed () -> ()", "thunk for closure", "cocoa"),
    ("specialized thunk for @callee_guaranteed (@guaranteed [T1]) -> (@owned [T2])", "thunk for closure", "cocoa"),
    ("partial apply for thunk for @callee_guaranteed () -> (@error @owned Error)", "thunk for closure", "cocoa"),
    ("partial apply for thunk for @escaping @callee_guaranteed (@guaranteed SomeType, @guaranteed [String : SomeType2], @guaranteed SomeType3) -> ()", "thunk for closure", "cocoa"),
    ("closure #1 (T1) in foo(bar: T2)", "closure in foo", "cocoa"),
    ("partial apply for closure #1 () in closure #2 (T1) in f1(_: T2, arg: T3)", "closure in f1", "cocoa"),
    ("+[foo:(bar)]", "+[foo:(bar)]", "objc"),
    ("[foo:(bar)]", "[foo:(bar)]", "objc"),
    ("-[foo:(bar)]", "-[foo:(bar)]", "objc"),
    ("(anonymous namespace)::foo(int)", "(anonymous namespace)::foo", "native"),
    ("foo::bar::foo(int)", "foo::bar::foo", "native"),
];

#[test]
fn trims_like_sentry() {
    let failures: Vec<String> = CASES
        .iter()
        .filter_map(|(input, expected, platform)| {
            let got = trim_function_name(input, platform);
            (got != *expected)
                .then(|| format!("{platform}: {input:?}\n   got {got:?}\n  want {expected:?}"))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        CASES.len(),
        failures.join("\n")
    );
}
