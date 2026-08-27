#!/usr/bin/env ruby
# frozen_string_literal: true

require "yaml"

kind, canonical_path, ci_path = ARGV
abort "usage: verify-manifest-parity.rb KIND CANONICAL CI" unless kind && canonical_path && ci_path

canonical = YAML.load_file(canonical_path)
ci = YAML.load_file(ci_path)

expected_app_id = kind == "sandbox" ? "io.github.stranmor.MediaStudio" : "io.github.stranmor.MediaStudio.HostIntegration"
abort "unexpected #{kind} app id" unless canonical["app-id"] == expected_app_id
expected_source = { "type" => "dir", "path" => "../.." }
abort "#{kind} canonical source must use the checked-out candidate" unless canonical.dig("modules", 0, "sources", 0) == expected_source

if kind == "sandbox"
  expected_finish_args = [
    "--share=ipc",
    "--socket=wayland",
    "--socket=fallback-x11",
    "--device=dri",
    "--filesystem=xdg-config/media-studio:create",
    "--filesystem=xdg-download",
    "--filesystem=xdg-documents",
    "--filesystem=xdg-music",
    "--filesystem=xdg-pictures",
    "--filesystem=xdg-videos"
  ].sort
  abort "sandbox finish-args contract drifted" unless canonical.fetch("finish-args").sort == expected_finish_args
  codecs = canonical.dig("add-extensions", "org.freedesktop.Platform.codecs-extra")
  expected_codecs = {
    "directory" => "lib/codecs-extra",
    "version" => "25.08-extra",
    "add-ld-path" => "lib"
  }
  abort "sandbox codecs-extra contract drifted" unless codecs == expected_codecs
  commands = canonical.dig("modules", 0, "build-commands") || []
  abort "sandbox must create codecs-extra mountpoint" unless commands.include?("install -d /app/lib/codecs-extra")
  abort "sandbox must create app bin directory" unless commands.include?("install -d /app/bin")
  abort "sandbox must expose runtime ffmpeg" unless commands.include?("ln -srv /usr/bin/ffmpeg /app/bin/ffmpeg")
  abort "sandbox must expose runtime ffprobe" unless commands.include?("ln -srv /usr/bin/ffprobe /app/bin/ffprobe")
  abort "sandbox desktop must be explicitly sandboxed" unless File.read("packaging/flatpak/io.github.stranmor.MediaStudio.desktop").include?("Comment=Convert media with verified profiles")
else
  expected_finish_args = [
    "--share=ipc",
    "--socket=session-bus",
    "--device=dri",
    "--filesystem=home",
    "--filesystem=host",
    "--filesystem=xdg-config/media-studio:create",
    "--filesystem=xdg-config/systemd/user",
    "--filesystem=xdg-data/kio",
    "--talk-name=org.freedesktop.systemd1",
    "--talk-name=org.freedesktop.Flatpak"
  ].sort
  abort "host integration finish-args contract drifted" unless canonical.fetch("finish-args").sort == expected_finish_args
  commands = canonical.dig("modules", 0, "build-commands") || []
  abort "host integration wrapper contract missing" unless commands.any? { |line| line.include?("media-studio-host-tool") }
  abort "host integration must expose KIO data" unless canonical.fetch("finish-args").include?("--filesystem=xdg-data/kio")
  abort "host integration desktop must state its boundary" unless File.read("packaging/flatpak/io.github.stranmor.MediaStudio.HostIntegration.desktop").include?("Host Integration")
end

normalized = Marshal.load(Marshal.dump(canonical))
abort "#{kind} CI manifest drifted from canonical manifest" unless normalized == ci

puts "manifest_contract=verified kind=#{kind}"
