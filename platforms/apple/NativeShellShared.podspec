# CocoaPods distribution of the platform-neutral Swift shared by the Apple
# renderers (protocol, batch codec, controlled input, JSON, SVG paths). The
# NativeShell pod depends on it; module name NativeShellShared, as with SwiftPM.

Pod::Spec.new do |s|
  s.name = 'NativeShellShared'
  s.module_name = 'NativeShellShared'
  s.version = '0.1.0'
  s.summary = 'The platform-neutral part of the native-shell Apple renderers.'
  s.description = <<~DESC
    The mutation protocol of native-shell and its validating batch decoder, the
    controlled-input protocol, JSON helpers and an SVG path parser, shared by
    the UIKit and AppKit renderers.
  DESC
  # TODO: set the real repository URL before publishing.
  s.homepage = 'https://example.invalid/TODO-native-shell'
  s.license = { :type => 'Apache-2.0', :file => '../../LICENSE' }
  s.authors = { 'native-shell contributors' => 'TODO@example.invalid' }
  s.source = { :git => 'https://example.invalid/TODO-native-shell.git', :tag => "v#{s.version}" }

  s.ios.deployment_target = '14.0'
  s.osx.deployment_target = '12.0'
  s.swift_versions = ['5.9']

  s.source_files = 'NativeShellShared/Sources/NativeShellShared/**/*.swift'
  s.frameworks = 'Foundation', 'CoreGraphics'
end
