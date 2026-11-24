/*
 * The apps in the flow, each with its `platformicons` mark.
 *
 * The package's `<PlatformIcon>` `require`s every one of its SVGs, which
 * Astro's prerender cannot load, so the files are imported one by one: the
 * build ships these thirty-five, not the whole set. The name on the right is
 * what `PlatformIcon` resolves the platform to (`PLATFORM_TO_ICON`, falling
 * back to the language).
 */
import actix from 'platformicons/svg/actix.svg?url';
import android from 'platformicons/svg/android.svg?url';
import angular from 'platformicons/svg/angularjs.svg?url';
import apple from 'platformicons/svg/apple.svg?url';
import bun from 'platformicons/svg/bun.svg?url';
import capacitor from 'platformicons/svg/capacitor.svg?url';
import celery from 'platformicons/svg/celery.svg?url';
import deno from 'platformicons/svg/deno.svg?url';
import django from 'platformicons/svg/django.svg?url';
import dotnet from 'platformicons/svg/dotnet.svg?url';
import electron from 'platformicons/svg/electron.svg?url';
import elixir from 'platformicons/svg/elixir.svg?url';
import express from 'platformicons/svg/express.svg?url';
import fastapi from 'platformicons/svg/fastapi.svg?url';
import flask from 'platformicons/svg/flask.svg?url';
import flutter from 'platformicons/svg/flutter.svg?url';
import gin from 'platformicons/svg/gin.svg?url';
import go from 'platformicons/svg/go.svg?url';
import java from 'platformicons/svg/java.svg?url';
import kotlin from 'platformicons/svg/kotlin.svg?url';
import laravel from 'platformicons/svg/laravel.svg?url';
import nestjs from 'platformicons/svg/nestjs.svg?url';
import nextjs from 'platformicons/svg/nextjs.svg?url';
import python from 'platformicons/svg/python.svg?url';
import rails from 'platformicons/svg/rails.svg?url';
import react from 'platformicons/svg/react.svg?url';
import reactNative from 'platformicons/svg/react-native.svg?url';
import remix from 'platformicons/svg/remix.svg?url';
import rust from 'platformicons/svg/rust.svg?url';
import sinatra from 'platformicons/svg/sinatra.svg?url';
import springboot from 'platformicons/svg/springboot.svg?url';
import svelte from 'platformicons/svg/svelte.svg?url';
import symfony from 'platformicons/svg/symfony.svg?url';
import unity from 'platformicons/svg/unity.svg?url';
import vue from 'platformicons/svg/vue.svg?url';

/** In the order the first twenty take their seats. */
export const ICONS: Record<string, string> = {
  'javascript-react': react,
  'python-django': django,
  'node-express': express,
  go,
  rust,
  'ruby-rails': rails,
  'php-laravel': laravel,
  'java-spring-boot': springboot,
  dotnet,
  'apple-ios': apple,
  android,
  'javascript-nextjs': nextjs,
  'python-fastapi': fastapi,
  'javascript-vue': vue,
  elixir,
  flutter,
  'react-native': reactNative,
  'python-flask': flask,
  'javascript-angular': angular,
  kotlin,
  'php-symfony': symfony,
  'node-nestjs': nestjs,
  'javascript-svelte': svelte,
  unity,
  'javascript-remix': remix,
  bun,
  deno,
  'go-gin': gin,
  'rust-actix': actix,
  'python-celery': celery,
  'ruby-sinatra': sinatra,
  electron,
  capacitor,
  java,
  python,
};

export const PLATFORMS = Object.keys(ICONS);
