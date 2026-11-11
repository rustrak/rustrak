/**
 * Per-platform SDK setup snippets.
 *
 * Extracted from Sentry's own `static/app/gettingStartedDocs/<id>/` at
 * commit a32a33a5 and reduced to plain error reporting: no tracing, profiling,
 * logs, replay or metrics branches. Nothing here is written from memory; every
 * snippet traces back to a file in that tree.
 *
 * The literal `__DSN__` is substituted with the project's real DSN at render
 * time, see `renderSnippet`.
 *
 * Coverage is deliberately partial. A platform with no entry renders the DSN
 * panel plus a link to the official docs and no snippet at all. It must never
 * borrow another platform's snippet: Sentry is stricter still and renders an
 * explicit "documentation for this platform is currently unavailable" error
 * (`sdkDocumentation.tsx`), reserving the DSN-plus-docs panel for platforms it
 * has deprecated (`deprecatedPlatformInfo.tsx`). A missing entry is a thinner
 * page, never a wrong one.
 */
export interface PlatformSnippet {
  /**
   * Package-manager command, or a dependency declaration for ecosystems that
   * install via a manifest (Cargo, pub, Mix, Gradle, SPM).
   *
   * Absent for platforms whose only documented install path is Sentry's
   * `sentry-wizard`. That tool takes `--org` and `--project` slugs and
   * configures against a Sentry backend, so it cannot set a project up against
   * Rustrak. Sentry's own "Manual Configuration" section for those platforms
   * contains no snippet either: it shows the DSN and links to the docs, which
   * is what we do.
   */
  install?: string;
  /** SDK initialisation, containing the literal `__DSN__`. */
  configure: string;
  /** Prism language id for the highlighter. */
  language: string;
}

export const PLATFORM_SNIPPETS: Record<string, PlatformSnippet> = {
  android: {
    configure: `import io.sentry.android.core.SentryAndroid;
import android.app.Application;

public class MyApplication extends Application {
  public void onCreate() {
    super.onCreate();
    SentryAndroid.init(this, options -> {
      options.setDsn("__DSN__");
    });
  }
}`,
    language: 'java',
  },
  'apple-ios': {
    configure: `import Sentry

SentrySDK.start { options in
    options.dsn = "__DSN__"
}`,
    language: 'swift',
  },
  'apple-macos': {
    install: `.package(url: "https://github.com/getsentry/sentry-cocoa", from: "8.9.3"),`,
    configure: `import Sentry

func applicationDidFinishLaunching(_ aNotification: Notification) {

    SentrySDK.start { options in
        options.dsn = "__DSN__"
        options.debug = true // Enabling debug when first installing is always helpful

        // Adds IP for users.
        // For more information, visit: https://docs.sentry.io/platforms/apple/data-management/data-collected/
        options.sendDefaultPii = true
    }
}`,
    language: 'swift',
  },
  bun: {
    install: `bun add @sentry/bun`,
    configure: `import * as Sentry from "@sentry/bun";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  capacitor: {
    install: `npm install --save @sentry/capacitor @sentry/angular@^7`,
    configure: `import * as Sentry from '@sentry/capacitor';
import * as SentryAngular from '@sentry/angular';

Sentry.init({
  dsn: "__DSN__"
},
// Forward the init method from @sentry/angular
SentryAngular.init
);`,
    language: 'javascript',
  },
  cordova: {
    install: `cordova plugin add sentry-cordova`,
    configure: `onDeviceReady: function() {
  var Sentry = cordova.require('sentry-cordova.Sentry');
  Sentry.init({ dsn: '__DSN__' });
}`,
    language: 'javascript',
  },
  dart: {
    install: `sentry: ^9.6.0`,
    configure: `import 'package:sentry/sentry.dart';

Future<void> main() async {
  await Sentry.init((options) {
    options.dsn = '__DSN__';
    // Adds request headers and IP for users,
    // visit: https://docs.sentry.io/platforms/dart/data-management/data-collected/ for more info
    options.sendDefaultPii = true;
  });
}`,
    language: 'dart',
  },
  deno: {
    install: `import * as Sentry from "npm:@sentry/deno";`,
    configure: `import * as Sentry from "npm:@sentry/deno";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  dotnet: {
    install: `dotnet add package Sentry`,
    configure: `using Sentry;

SentrySdk.Init(options =>
{
    // A Sentry Data Source Name (DSN) is required.
    options.Dsn = "__DSN__";

    // When debug is enabled, the Sentry client will emit detailed debugging information to the console.
    options.Debug = true;

    // This option is recommended. It enables Sentry's "Release Health" feature.
    options.AutoSessionTracking = true;
});`,
    language: 'csharp',
  },
  'dotnet-aspnet': {
    install: `Install-Package Sentry.AspNet`,
    configure: `public class MvcApplication : HttpApplication
{
    private IDisposable _sentry;

    protected void Application_Start()
    {
        // Initialize Sentry to capture AppDomain unhandled exceptions and more.
        _sentry = SentrySdk.Init(o =>
        {
            o.AddAspNet();
            o.Dsn = "__DSN__";
            // When configuring for the first time, to see what the SDK is doing:
            o.Debug = true;
        });
    }
}`,
    language: 'csharp',
  },
  'dotnet-aspnetcore': {
    install: `dotnet add package Sentry.AspNetCore`,
    configure: `public static IHostBuilder CreateHostBuilder(string[] args) =>
  Host.CreateDefaultBuilder(args)
      .ConfigureWebHostDefaults(webBuilder =>
      {
          // Add the following line:
          webBuilder.UseSentry(o =>
          {
              o.Dsn = "__DSN__";
              // When configuring for the first time, to see what the SDK is doing:
              o.Debug = true;
          });
      });`,
    language: 'csharp',
  },
  'dotnet-awslambda': {
    install: `dotnet add package Sentry.AspNetCore`,
    configure: `public class LambdaEntryPoint : Amazon.Lambda.AspNetCoreServer.APIGatewayProxyFunction
{
    protected override void Init(IWebHostBuilder builder)
    {
        builder
            // Add Sentry
            .UseSentry(o =>
            {
              o.Dsn = "__DSN__";
              // When configuring for the first time, to see what the SDK is doing:
              o.Debug = true;
              // Required in Serverless environments
              o.FlushOnCompletedRequest = true;
            });
    }
}`,
    language: 'csharp',
  },
  'dotnet-gcpfunctions': {
    install: `dotnet add package Sentry.Google.Cloud.Functions`,
    configure: `{
  "Sentry": {
    "Dsn": "__DSN__",
    "SendDefaultPii": true,
    "Debug": true,
    "MaxRequestBodySize": "Always"
  }
}`,
    language: 'json',
  },
  'dotnet-maui': {
    install: `dotnet add package Sentry.Maui`,
    configure: `public static MauiApp CreateMauiApp()
{
  var builder = MauiApp.CreateBuilder();
  builder
    .UseMauiApp<App>()

    // Add this section anywhere on the builder:
    .UseSentry(options => {
      // The DSN is the only required setting.
      options.Dsn = "__DSN__";

      // Use debug mode if you want to see what the SDK is doing.
      options.Debug = true;
  })

  // ... the remainder of your MAUI app setup

  return builder.Build();
}`,
    language: 'csharp',
  },
  'dotnet-winforms': {
    install: `dotnet add package Sentry`,
    configure: `static class Program
{
    [STAThread]
    static void Main()
    {
        // Init the Sentry SDK
        SentrySdk.Init(o =>
        {
            // Tells which project in Sentry to send events to:
            o.Dsn = "__DSN__";
            // When configuring for the first time, to see what the SDK is doing:
            o.Debug = true;
        });
        // Configure WinForms to throw exceptions so Sentry can capture them.
        Application.SetUnhandledExceptionMode(UnhandledExceptionMode.ThrowException);
    }
}`,
    language: 'csharp',
  },
  'dotnet-wpf': {
    install: `dotnet add package Sentry`,
    configure: `using Sentry;

public partial class App : Application
{
    public App()
    {
        DispatcherUnhandledException += App_DispatcherUnhandledException;
        SentrySdk.Init(o =>
        {
            // Tells which project in Sentry to send events to:
            o.Dsn = "__DSN__";
            // When configuring for the first time, to see what the SDK is doing:
            o.Debug = true;
        });
    }
}`,
    language: 'csharp',
  },
  'dotnet-xamarin': {
    install: `Install-Package Sentry.Xamarin`,
    configure: `public class MainActivity : global::Xamarin.Forms.Platform.Android.FormsAppCompatActivity
{
    protected override void OnCreate(Bundle savedInstanceState)
    {
        SentryXamarin.Init(options =>
        {
            // Tells which project in Sentry to send events to:
            options.Dsn = "__DSN__";
            // When configuring for the first time, to see what the SDK is doing:
            options.Debug = true;
            // If you installed Sentry.Xamarin.Forms:
            options.AddXamarinFormsIntegration();
        });
    }
}`,
    language: 'csharp',
  },
  electron: {
    install: `npm install --save @sentry/electron`,
    configure: `import * as Sentry from "@sentry/electron";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  elixir: {
    install: `{:sentry, "~> 10.2.0"}`,
    configure: `config :sentry,
  dsn: "__DSN__",
  environment_name: Mix.env(),
  enable_source_code_context: true,
  root_source_code_paths: [File.cwd!()]`,
    language: 'elixir',
  },
  // Flutter is a wizard platform, so its `onboarding.tsx` carries no configure
  // snippet to extract. Both fields below come from `flutter/sessionReplay.tsx`
  // instead (install at its `getManualInstallSnippet`, the `appRunner` shape at
  // its replay snippet), with the replay-specific options dropped.
  flutter: {
    install: `sentry_flutter: ^9.6.0`,
    configure: `import 'package:flutter/widgets.dart';
import 'package:sentry_flutter/sentry_flutter.dart';

Future<void> main() async {
  await SentryFlutter.init(
    (options) {
      options.dsn = '__DSN__';
    },
    appRunner: () => runApp(
      SentryWidget(
        child: MyApp(),
      ),
    ),
  );
}`,
    language: 'dart',
  },
  go: {
    install: `go get github.com/getsentry/sentry-go`,
    configure: `package main

import (
  "log"

  "github.com/getsentry/sentry-go"
)

func main() {
  err := sentry.Init(sentry.ClientOptions{
    Dsn: "__DSN__",
  })
  if err != nil {
    log.Fatalf("sentry.Init: %s", err)
  }
}`,
    language: 'go',
  },
  'go-echo': {
    install: `go get github.com/getsentry/sentry-go/echo`,
    configure: `import (
  "fmt"
  "net/http"

  "github.com/getsentry/sentry-go"
  sentryecho "github.com/getsentry/sentry-go/echo"
  "github.com/labstack/echo/v4"
  "github.com/labstack/echo/v4/middleware"
)

// To initialize Sentry's handler, you need to initialize Sentry itself beforehand
if err := sentry.Init(sentry.ClientOptions{
  Dsn: "__DSN__",
}); err != nil {
  fmt.Printf("Sentry initialization failed: %v\\n", err)
}`,
    language: 'go',
  },
  'go-fasthttp': {
    install: `go get github.com/getsentry/sentry-go/fasthttp`,
    configure: `import (
  "fmt"
  "net/http"

  "github.com/getsentry/sentry-go"
  sentryfasthttp "github.com/getsentry/sentry-go/fasthttp"
)

// To initialize Sentry's handler, you need to initialize Sentry itself beforehand
if err := sentry.Init(sentry.ClientOptions{
  Dsn: "__DSN__",
}); err != nil {
  fmt.Printf("Sentry initialization failed: %v\\n", err)
}`,
    language: 'go',
  },
  'go-fiber': {
    install: `go get github.com/getsentry/sentry-go/fiber`,
    configure: `import (
  "fmt"
  "net/http"

  "github.com/getsentry/sentry-go"
  sentryfiber "github.com/getsentry/sentry-go/fiber"
)

// To initialize Sentry's handler, you need to initialize Sentry itself beforehand
if err := sentry.Init(sentry.ClientOptions{
  Dsn: "__DSN__",
}); err != nil {
  fmt.Printf("Sentry initialization failed: %v\\n", err)
}`,
    language: 'go',
  },
  'go-gin': {
    install: `go get github.com/getsentry/sentry-go/gin`,
    configure: `import (
  "fmt"
  "net/http"

  "github.com/getsentry/sentry-go"
  sentrygin "github.com/getsentry/sentry-go/gin"
  "github.com/gin-gonic/gin"
)

// To initialize Sentry's handler, you need to initialize Sentry itself beforehand
if err := sentry.Init(sentry.ClientOptions{
  Dsn: "__DSN__",
}); err != nil {
  fmt.Printf("Sentry initialization failed: %v\\n", err)
}`,
    language: 'go',
  },
  'go-http': {
    install: `go get github.com/getsentry/sentry-go/http`,
    configure: `import (
  "fmt"
  "net/http"

  "github.com/getsentry/sentry-go"
  sentryhttp "github.com/getsentry/sentry-go/http"
)

// To initialize Sentry's handler, you need to initialize Sentry itself beforehand
if err := sentry.Init(sentry.ClientOptions{
  Dsn: "__DSN__",
}); err != nil {
  fmt.Printf("Sentry initialization failed: %v\\n", err)
}`,
    language: 'go',
  },
  'go-iris': {
    install: `go get github.com/getsentry/sentry-go/iris`,
    configure: `import (
  "fmt"

  "github.com/getsentry/sentry-go"
  sentryiris "github.com/getsentry/sentry-go/iris"
  "github.com/kataras/iris/v12"
)

// To initialize Sentry's handler, you need to initialize Sentry itself beforehand
if err := sentry.Init(sentry.ClientOptions{
  Dsn: "__DSN__",
}); err != nil {
  fmt.Printf("Sentry initialization failed: %v\\n", err)
}`,
    language: 'go',
  },
  'go-negroni': {
    install: `go get github.com/getsentry/sentry-go/negroni`,
    configure: `import (
  "fmt"
  "net/http"

  "github.com/getsentry/sentry-go"
  sentrynegroni "github.com/getsentry/sentry-go/negroni"
  "github.com/urfave/negroni"
)

// To initialize Sentry's handler, you need to initialize Sentry itself beforehand
if err := sentry.Init(sentry.ClientOptions{
  Dsn: "__DSN__",
}); err != nil {
  fmt.Printf("Sentry initialization failed: %v\\n", err)
}`,
    language: 'go',
  },
  java: {
    install: `id "io.sentry.jvm.gradle" version "3.12.0"`,
    configure: `import io.sentry.Sentry;

Sentry.init(options -> {
  options.setDsn("__DSN__");

  // Add data like request headers and IP for users,
  // see https://docs.sentry.io/platforms/java/data-management/data-collected/ for more info
  options.setSendDefaultPii(true);
  // When first trying Sentry it's good to see what the SDK is doing:
  options.setDebug(true);
});`,
    language: 'java',
  },
  'java-log4j2': {
    install: `id "io.sentry.jvm.gradle" version "3.12.0"`,
    configure: `<?xml version="1.0" encoding="UTF-8"?>
<Configuration status="warn" packages="org.apache.logging.log4j.core,io.sentry.log4j2">
    <Appenders>
        <Console name="Console" target="SYSTEM_OUT">
            <PatternLayout pattern="%d{HH:mm:ss.SSS} [%t] %-5level %logger{36} - %msg%n"/>
        </Console>
        <Sentry name="Sentry"
                dsn=__DSN__>
    </Appenders>
    <Loggers>
        <Root level="info">
            <AppenderRef ref="Sentry"/>
            <AppenderRef ref="Console"/>
        </Root>
    </Loggers>
</Configuration>`,
    language: 'xml',
  },
  'java-logback': {
    install: `id "io.sentry.jvm.gradle" version "3.12.0"`,
    configure: `<configuration>
  <!-- Configure the Sentry appender -->
  <appender name="Sentry" class="io.sentry.logback.SentryAppender">
    <options>
      <dsn>__DSN__</dsn>
      <!-- Add data like request headers and IP for users, see https://docs.sentry.io/platforms/java/guides/logback/data-management/data-collected/ for more info -->
      <sendDefaultPii>true</sendDefaultPii>
    </options>
  </appender>

  <root level="debug">
    <appender-ref ref="Sentry"/>
  </root>
</configuration>`,
    language: 'xml',
  },
  'java-spring': {
    install: `id "io.sentry.jvm.gradle" version "3.12.0"`,
    configure: `import io.sentry.spring7.EnableSentry;

@EnableSentry(
  dsn = "__DSN__",
  // Add data like request headers and IP for users,
  // see https://docs.sentry.io/platforms/java/guides/spring/data-management/data-collected/ for more info
  sendDefaultPii = true
)
@Configuration
class SentryConfiguration {
}`,
    language: 'java',
  },
  'java-spring-boot': {
    install: `id "io.sentry.jvm.gradle" version "3.12.0"`,
    configure: `sentry.dsn=__DSN__
# Add data like request headers and IP for users,
# see https://docs.sentry.io/platforms/java/guides/spring-boot/data-management/data-collected/ for more info
sentry.send-default-pii=true`,
    language: 'bash',
  },
  javascript: {
    install: `npm install --save @sentry/browser`,
    configure: `import * as Sentry from "@sentry/browser";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'javascript-angular': {
    install: `npm install --save @sentry/angular`,
    configure: `import { bootstrapApplication } from '@angular/platform-browser';
import * as Sentry from "@sentry/angular";

import { appConfig } from './app/app.config';
import { AppComponent } from './app/app.component';

Sentry.init({
  dsn: "__DSN__",
});

bootstrapApplication(appConfig, AppComponent)
  .catch((err) => console.error(err));`,
    language: 'javascript',
  },
  'javascript-astro': {
    install: `npx astro add @sentry/astro`,
    configure: `import * as Sentry from "@sentry/astro";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'javascript-ember': {
    install: `ember install @sentry/ember`,
    configure: `import * as Sentry from "@sentry/ember";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'javascript-gatsby': {
    install: `npm install --save @sentry/gatsby`,
    configure: `import * as Sentry from "@sentry/gatsby";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'javascript-nextjs': {
    configure: `import * as Sentry from "@sentry/nextjs";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'javascript-nuxt': {
    install: `npm install --save @sentry/nuxt`,
    configure: `import * as Sentry from "@sentry/nuxt";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'javascript-react': {
    install: `npm install --save @sentry/react`,
    configure: `import * as Sentry from "@sentry/react";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'javascript-react-router': {
    configure: `import * as Sentry from "@sentry/react-router";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'javascript-remix': {
    configure: `import * as Sentry from "@sentry/remix";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'javascript-solid': {
    install: `npm install --save @sentry/solid`,
    configure: `import * as Sentry from "@sentry/solid";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'javascript-solidstart': {
    install: `npm install --save @sentry/solidstart`,
    configure: `import * as Sentry from "@sentry/solidstart";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'javascript-svelte': {
    install: `npm install --save @sentry/svelte`,
    configure: `import "./app.css";
import App from "./App.svelte";

import * as Sentry from "@sentry/svelte";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'javascript-sveltekit': {
    configure: `import * as Sentry from "@sentry/sveltekit";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'javascript-tanstackstart-react': {
    install: `npm install --save @sentry/tanstackstart-react`,
    configure: `import * as Sentry from "@sentry/tanstackstart-react";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'typescript',
  },
  'javascript-vue': {
    install: `npm install --save @sentry/vue`,
    configure: `import {createApp} from "vue";
import * as Sentry from "@sentry/vue";

const app = createApp({
  // ...
});

Sentry.init({
  app,
  dsn: "__DSN__",
});

app.mount("#app");`,
    language: 'javascript',
  },
  kotlin: {
    install: `id "io.sentry.jvm.gradle" version "3.12.0"`,
    configure: `import io.sentry.Sentry

Sentry.init { options ->
  options.dsn = "__DSN__"
  // When first trying Sentry it's good to see what the SDK is doing:
  options.isDebug = true
}`,
    language: 'kotlin',
  },
  node: {
    install: `npm install @sentry/node --save`,
    configure: `// Import with \`import * as Sentry from "@sentry/node"\` if you are using ESM
const Sentry = require("@sentry/node");

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'node-awslambda': {
    install: `npm install @sentry/aws-serverless --save`,
    configure: `NODE_OPTIONS="--import @sentry/aws-serverless/awslambda-auto"
SENTRY_DSN="__DSN__"`,
    language: 'bash',
  },
  'node-azurefunctions': {
    install: `npm install @sentry/node --save`,
    configure: `// Import with \`import * as Sentry from "@sentry/node"\` if you are using ESM
const Sentry = require("@sentry/node");

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'node-cloudflare-pages': {
    install: `npm install @sentry/cloudflare --save`,
    configure: `import * as Sentry from "@sentry/cloudflare";

export const onRequest = [
  // Make sure Sentry is the first middleware
  Sentry.sentryPagesPlugin((context) => ({
    dsn: "__DSN__",
  })),
  // Add more middlewares here
];`,
    language: 'javascript',
  },
  'node-cloudflare-workers': {
    install: `npm install @sentry/cloudflare --save`,
    configure: `import * as Sentry from "@sentry/cloudflare";

export default Sentry.withSentry(
  (env: Env) => ({
    dsn: "__DSN__",
  }),
  {
    async fetch(request, env, ctx) {
      return new Response('Hello World!');
    },
  } satisfies ExportedHandler<Env>,
);`,
    language: 'typescript',
  },
  'node-connect': {
    install: `npm install @sentry/node --save`,
    configure: `// Import with \`import * as Sentry from "@sentry/node"\` if you are using ESM
const Sentry = require("@sentry/node");

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'node-express': {
    install: `npm install @sentry/node --save`,
    configure: `// Import with \`import * as Sentry from "@sentry/node"\` if you are using ESM
const Sentry = require("@sentry/node");

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'node-fastify': {
    install: `npm install @sentry/node --save`,
    configure: `// Import with \`import * as Sentry from "@sentry/node"\` if you are using ESM
const Sentry = require("@sentry/node");

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'node-gcpfunctions': {
    install: `npm install @sentry/google-cloud-serverless --save`,
    configure: `// Import with \`import * as Sentry from "@sentry/google-cloud-serverless"\` if you are using ESM
const Sentry = require("@sentry/google-cloud-serverless");

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'node-hapi': {
    install: `npm install @sentry/node --save`,
    configure: `// Import with \`import * as Sentry from "@sentry/node"\` if you are using ESM
const Sentry = require("@sentry/node");

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'node-hono': {
    install: `npm install @sentry/hono @sentry/cloudflare --save`,
    configure: `import { Hono } from "hono";
import { sentry } from "@sentry/hono/cloudflare";

const app = new Hono();

app.use(
  sentry(app, {
    dsn: "__DSN__",
  }),
);

// Your routes here
app.get("/", (c) => {
  return c.text("Hello Hono!");
});

export default app;`,
    language: 'javascript',
  },
  'node-koa': {
    install: `npm install @sentry/node --save`,
    configure: `// Import with \`import * as Sentry from "@sentry/node"\` if you are using ESM
const Sentry = require("@sentry/node");

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  'node-nestjs': {
    install: `npm install @sentry/nestjs --save`,
    configure: `// Import with \`const Sentry = require("@sentry/nestjs");\` if you are using CJS
import * as Sentry from "@sentry/nestjs"

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  php: {
    install: `composer require sentry/sentry`,
    configure: `\\Sentry\\init([
  'dsn' => '__DSN__',
]);`,
    language: 'php',
  },
  'php-laravel': {
    install: `composer require sentry/sentry-laravel`,
    configure: `php artisan sentry:publish --dsn=__DSN__`,
    language: 'bash',
  },
  'php-symfony': {
    install: `composer require sentry/sentry-symfony`,
    configure: `###> sentry/sentry-symfony ###
SENTRY_DSN="__DSN__"
###< sentry/sentry-symfony ###`,
    language: 'bash',
  },
  powershell: {
    install: `Install-Module -Name Sentry -Repository PSGallery -RequiredVersion 0.0.2 -Force`,
    configure: `# You need to import the module once in a script.
Import-Module Sentry

Start-Sentry {
    $_.Dsn = '__DSN__'
}`,
    language: 'bash',
  },
  python: {
    install: `pip install "sentry-sdk"`,
    configure: `import sentry_sdk

sentry_sdk.init(
    dsn="__DSN__",
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-aiohttp': {
    install: `pip install "sentry-sdk"`,
    configure: `from aiohttp import web

import sentry_sdk

sentry_sdk.init(
    dsn="__DSN__",
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-asgi': {
    install: `pip install "sentry-sdk"`,
    configure: `import sentry_sdk
from sentry_sdk.integrations.asgi import SentryAsgiMiddleware

from myapp import asgi_app

sentry_sdk.init(
    dsn="__DSN__",
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-awslambda': {
    install: `pip install "sentry-sdk"`,
    configure: `import sentry_sdk
from sentry_sdk.integrations.aws_lambda import AwsLambdaIntegration

sentry_sdk.init(
    dsn="__DSN__",
    integrations=[AwsLambdaIntegration()],
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-bottle': {
    install: `pip install "sentry-sdk" "bottle"`,
    configure: `import sentry_sdk

sentry_sdk.init(
    dsn="__DSN__",
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-celery': {
    install: `pip install "sentry-sdk" "celery"`,
    configure: `import sentry_sdk

sentry_sdk.init(
    dsn="__DSN__",
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-chalice': {
    install: `pip install "sentry-sdk" "chalice"`,
    configure: `import sentry_sdk
from chalice import Chalice

from sentry_sdk.integrations.chalice import ChaliceIntegration

sentry_sdk.init(
    dsn="__DSN__",
    integrations=[ChaliceIntegration()],
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-django': {
    install: `pip install "sentry-sdk" "django"`,
    configure: `import sentry_sdk

sentry_sdk.init(
    dsn="__DSN__",
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-falcon': {
    install: `pip install "sentry-sdk" "falcon"`,
    configure: `import falcon
import sentry_sdk

sentry_sdk.init(
    dsn="__DSN__",
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-fastapi': {
    install: `pip install "sentry-sdk" "fastapi"`,
    configure: `from fastapi import FastAPI
import sentry_sdk

sentry_sdk.init(
    dsn="__DSN__",
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-flask': {
    install: `pip install "sentry-sdk" "flask"`,
    configure: `import sentry_sdk
from flask import Flask

sentry_sdk.init(
    dsn="__DSN__",
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-gcpfunctions': {
    install: `pip install "sentry-sdk"`,
    configure: `import sentry_sdk
from sentry_sdk.integrations.gcp import GcpIntegration

sentry_sdk.init(
    dsn="__DSN__",
    integrations=[GcpIntegration()],
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-litestar': {
    install: `pip install "sentry-sdk" "litestar"`,
    configure: `import sentry_sdk

sentry_sdk.init(
    dsn="__DSN__",
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-pyramid': {
    install: `pip install "sentry-sdk"`,
    configure: `from pyramid.config import Configurator
import sentry_sdk

sentry_sdk.init(
    dsn="__DSN__",
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-quart': {
    install: `pip install "sentry-sdk" "quart"`,
    configure: `import sentry_sdk
from sentry_sdk.integrations.quart import QuartIntegration
from quart import Quart

sentry_sdk.init(
    dsn="__DSN__",
    integrations=[QuartIntegration()],
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-rq': {
    install: `pip install "sentry-sdk" "rq"`,
    configure: `import sentry_sdk

sentry_sdk.init(
    dsn="__DSN__",
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-sanic': {
    install: `pip install "sentry-sdk" "sanic"`,
    configure: `from sanic import Sanic
import sentry_sdk

sentry_sdk.init(
    dsn="__DSN__",
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-serverless': {
    install: `pip install "sentry-sdk"`,
    configure: `import sentry_sdk
from sentry_sdk.integrations.serverless import serverless_function

sentry_sdk.init(
    dsn="__DSN__",
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-starlette': {
    install: `pip install "sentry-sdk" "starlette"`,
    configure: `from starlette.applications import Starlette
import sentry_sdk

sentry_sdk.init(
    dsn="__DSN__",
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-tornado': {
    install: `pip install "sentry-sdk" "tornado"`,
    configure: `import sentry_sdk

sentry_sdk.init(
    dsn="__DSN__",
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-tryton': {
    install: `pip install "sentry-sdk"`,
    configure: `import sentry_sdk
from sentry_sdk.integrations.trytond import TrytondWSGIIntegration

sentry_sdk.init(
    dsn="__DSN__",
    integrations=[TrytondWSGIIntegration()],
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'python-wsgi': {
    install: `pip install "sentry-sdk"`,
    configure: `import sentry_sdk
from sentry_sdk.integrations.wsgi import SentryWsgiMiddleware

from my_wsgi_app import app

sentry_sdk.init(
    dsn="__DSN__",
    # Add data like request headers and IP for users,
    # see https://docs.sentry.io/platforms/python/data-management/data-collected/ for more info
    send_default_pii=True,
)`,
    language: 'python',
  },
  'react-native': {
    install: `npm install @sentry/react-native@latest`,
    configure: `import * as Sentry from "@sentry/react-native";

Sentry.init({
  dsn: "__DSN__",
});`,
    language: 'javascript',
  },
  ruby: {
    install: `gem "sentry-ruby"`,
    configure: `require 'sentry-ruby'

Sentry.init do |config|
  config.dsn = '__DSN__'

  # Add data like request headers and IP for users,
  # see https://docs.sentry.io/platforms/ruby/data-management/data-collected/ for more info
  config.send_default_pii = true
end`,
    language: 'ruby',
  },
  'ruby-rack': {
    install: `gem "sentry-ruby"`,
    configure: `require 'sentry-ruby'

Sentry.init do |config|
  config.dsn = '__DSN__'

  # Add data like request headers and IP for users,
  # see https://docs.sentry.io/platforms/ruby/data-management/data-collected/ for more info
  config.send_default_pii = true
end`,
    language: 'ruby',
  },
  'ruby-rails': {
    install: `gem "sentry-rails"`,
    configure: `Sentry.init do |config|
  config.dsn = '__DSN__'
  config.breadcrumbs_logger = [:active_support_logger, :http_logger]

  # Add data like request headers and IP for users,
  # see https://docs.sentry.io/platforms/ruby/data-management/data-collected/ for more info
  config.send_default_pii = true
end`,
    language: 'ruby',
  },
  rust: {
    install: `sentry = "0.42.0"`,
    configure: `let _guard = sentry::init(("__DSN__", sentry::ClientOptions {
  release: sentry::release_name!(),
  // Capture user IPs and potentially sensitive headers when using HTTP server integrations
  // see https://docs.sentry.io/platforms/rust/data-management/data-collected for more info
  send_default_pii: true,
  ..Default::default()
}));`,
    language: 'rust',
  },
};

/** Official Sentry documentation URL per platform, from Sentry's own platforms.tsx. */
export const PLATFORM_DOCS: Record<string, string> = {
  android: 'https://docs.sentry.io/platforms/android/',
  apple: 'https://docs.sentry.io/platforms/apple/',
  'apple-ios': 'https://docs.sentry.io/platforms/apple/',
  'apple-macos': 'https://docs.sentry.io/platforms/apple/',
  bun: 'https://docs.sentry.io/platforms/javascript/guides/bun/',
  capacitor: 'https://docs.sentry.io/platforms/javascript/guides/capacitor/',
  cordova: 'https://docs.sentry.io/platforms/javascript/guides/cordova/',
  dart: 'https://docs.sentry.io/platforms/dart/',
  deno: 'https://docs.sentry.io/platforms/javascript/guides/deno/',
  dotnet: 'https://docs.sentry.io/platforms/dotnet/',
  'dotnet-aspnet': 'https://docs.sentry.io/platforms/dotnet/guides/aspnet/',
  'dotnet-aspnetcore':
    'https://docs.sentry.io/platforms/dotnet/guides/aspnetcore/',
  'dotnet-awslambda':
    'https://docs.sentry.io/platforms/dotnet/guides/aws-lambda/',
  'dotnet-gcpfunctions':
    'https://docs.sentry.io/platforms/dotnet/guides/google-cloud-functions/',
  'dotnet-maui': 'https://docs.sentry.io/platforms/dotnet/guides/maui/',
  'dotnet-uwp': 'https://docs.sentry.io/platforms/dotnet/guides/uwp/',
  'dotnet-winforms': 'https://docs.sentry.io/platforms/dotnet/guides/winforms/',
  'dotnet-wpf': 'https://docs.sentry.io/platforms/dotnet/guides/wpf/',
  'dotnet-xamarin': 'https://docs.sentry.io/platforms/dotnet/guides/xamarin/',
  electron: 'https://docs.sentry.io/platforms/javascript/guides/electron/',
  elixir: 'https://docs.sentry.io/platforms/elixir/',
  flutter: 'https://docs.sentry.io/platforms/flutter/',
  ionic: 'https://docs.sentry.io/platforms/javascript/guides/capacitor/',
  java: 'https://docs.sentry.io/platforms/java/',
  'java-log4j2': 'https://docs.sentry.io/platforms/java/guides/log4j2/',
  'java-logback': 'https://docs.sentry.io/platforms/java/guides/logback/',
  'java-spring': 'https://docs.sentry.io/platforms/java/guides/spring/',
  'java-spring-boot':
    'https://docs.sentry.io/platforms/java/guides/spring-boot/',
  javascript: 'https://docs.sentry.io/platforms/javascript/',
  'javascript-angular':
    'https://docs.sentry.io/platforms/javascript/guides/angular/',
  'javascript-astro':
    'https://docs.sentry.io/platforms/javascript/guides/astro/',
  'javascript-ember':
    'https://docs.sentry.io/platforms/javascript/guides/ember/',
  'javascript-gatsby':
    'https://docs.sentry.io/platforms/javascript/guides/gatsby/',
  'javascript-nextjs':
    'https://docs.sentry.io/platforms/javascript/guides/nextjs/',
  'javascript-nuxt': 'https://docs.sentry.io/platforms/javascript/guides/nuxt/',
  'javascript-react':
    'https://docs.sentry.io/platforms/javascript/guides/react/',
  'javascript-react-router':
    'https://docs.sentry.io/platforms/javascript/guides/react-router/',
  'javascript-remix':
    'https://docs.sentry.io/platforms/javascript/guides/remix/',
  'javascript-solid':
    'https://docs.sentry.io/platforms/javascript/guides/solid/',
  'javascript-solidstart':
    'https://docs.sentry.io/platforms/javascript/guides/solidstart/',
  'javascript-svelte':
    'https://docs.sentry.io/platforms/javascript/guides/svelte/',
  'javascript-sveltekit':
    'https://docs.sentry.io/platforms/javascript/guides/sveltekit/',
  'javascript-tanstackstart-react':
    'https://docs.sentry.io/platforms/javascript/guides/tanstackstart-react/',
  'javascript-vue': 'https://docs.sentry.io/platforms/javascript/guides/vue/',
  kotlin: 'https://docs.sentry.io/platforms/kotlin/',
  minidump: 'https://docs.sentry.io/platforms/native/minidump/',
  native: 'https://docs.sentry.io/platforms/native/',
  'native-qt': 'https://docs.sentry.io/platforms/native/guides/qt/',
  'nintendo-switch': 'https://docs.sentry.io/platforms/nintendo-switch/',
  node: 'https://docs.sentry.io/platforms/javascript/guides/node',
  'node-awslambda':
    'https://docs.sentry.io/platforms/javascript/guides/aws-lambda/',
  'node-azurefunctions':
    'https://docs.sentry.io/platforms/javascript/guides/azure-functions/',
  'node-cloudflare-pages':
    'https://docs.sentry.io/platforms/javascript/guides/cloudflare/',
  'node-cloudflare-workers':
    'https://docs.sentry.io/platforms/javascript/guides/cloudflare/',
  'node-connect': 'https://docs.sentry.io/platforms/javascript/guides/connect/',
  'node-express': 'https://docs.sentry.io/platforms/javascript/guides/express/',
  'node-fastify': 'https://docs.sentry.io/platforms/javascript/guides/fastify/',
  'node-gcpfunctions':
    'https://docs.sentry.io/platforms/javascript/guides/gcp-functions/',
  'node-hapi': 'https://docs.sentry.io/platforms/javascript/guides/hapi/',
  'node-hono': 'https://docs.sentry.io/platforms/javascript/guides/hono/',
  'node-koa': 'https://docs.sentry.io/platforms/javascript/guides/koa/',
  'node-nestjs': 'https://docs.sentry.io/platforms/javascript/guides/nestjs/',
  other: 'https://docs.sentry.io/platforms/',
  php: 'https://docs.sentry.io/platforms/php/',
  'php-laravel': 'https://docs.sentry.io/platforms/php/guides/laravel/',
  'php-symfony': 'https://docs.sentry.io/platforms/php/guides/symfony/',
  playstation: 'https://docs.sentry.io/platforms/playstation/',
  powershell: 'https://docs.sentry.io/platforms/powershell/',
  python: 'https://docs.sentry.io/platforms/python/',
  'python-aiohttp': 'https://docs.sentry.io/platforms/python/guides/aiohttp/',
  'python-asgi': 'https://docs.sentry.io/platforms/python/guides/asgi/',
  'python-awslambda':
    'https://docs.sentry.io/platforms/python/guides/aws-lambda/',
  'python-bottle': 'https://docs.sentry.io/platforms/python/guides/bottle/',
  'python-celery': 'https://docs.sentry.io/platforms/python/guides/celery/',
  'python-chalice': 'https://docs.sentry.io/platforms/python/guides/chalice/',
  'python-django': 'https://docs.sentry.io/platforms/python/guides/django/',
  'python-falcon': 'https://docs.sentry.io/platforms/python/guides/falcon/',
  'python-fastapi': 'https://docs.sentry.io/platforms/python/guides/fastapi/',
  'python-flask': 'https://docs.sentry.io/platforms/python/guides/flask/',
  'python-gcpfunctions':
    'https://docs.sentry.io/platforms/python/guides/gcp-functions/',
  'python-litestar':
    'https://docs.sentry.io/platforms/python/integrations/litestar/',
  'python-pylons':
    'https://docs.sentry.io/platforms/python/legacy-sdk/integrations/pylons/',
  'python-pymongo': 'https://docs.sentry.io/platforms/python/guides/pymongo/',
  'python-pyramid': 'https://docs.sentry.io/platforms/python/pyramid/',
  'python-quart': 'https://docs.sentry.io/platforms/python/guides/quart/',
  'python-rq': 'https://docs.sentry.io/platforms/python/guides/rq/',
  'python-sanic': 'https://docs.sentry.io/platforms/python/guides/sanic/',
  'python-serverless':
    'https://docs.sentry.io/platforms/python/guides/serverless/',
  'python-starlette':
    'https://docs.sentry.io/platforms/python/guides/starlette/',
  'python-tornado': 'https://docs.sentry.io/platforms/python/guides/tornado/',
  'python-tryton': 'https://docs.sentry.io/platforms/python/guides/tryton/',
  'python-wsgi': 'https://docs.sentry.io/platforms/python/guides/wsgi/',
  'react-native': 'https://docs.sentry.io/platforms/react-native/',
  ruby: 'https://docs.sentry.io/platforms/ruby/',
  'ruby-rack': 'https://docs.sentry.io/platforms/ruby/guides/rack/',
  'ruby-rails': 'https://docs.sentry.io/platforms/ruby/guides/rails/',
  rust: 'https://docs.sentry.io/platforms/rust/',
  unity: 'https://docs.sentry.io/platforms/unity/',
  unreal: 'https://docs.sentry.io/platforms/unreal/',
  xbox: 'https://docs.sentry.io/platforms/xbox/',
};

/** Substitutes the project's DSN into a snippet. */
export function renderSnippet(snippet: string, dsn: string): string {
  return snippet.split('__DSN__').join(dsn);
}
