# CocoaPods distribution of the UIKit renderer, for apps that do not use
# Swift Package Manager. Build the core first: scripts/build-ios.sh produces
# Frameworks/NativeShellFFI.xcframework (static library, device + simulator).

Pod::Spec.new do |s|
  s.name = 'NativeShell'
  s.version = '0.1.0'
  s.summary = 'Run small JavaScript programs inside iOS apps, rendered with native UIKit views.'
  s.description = <<~DESC
    native-shell runs QuickJS programs whose Rust core turns render descriptions
    into a binary mutation stream; this pod is the UIKit renderer plus the host
    integration API (host modules and host components).
  DESC
  # TODO: set the real repository URL before publishing.
  s.homepage = 'https://example.invalid/TODO-native-shell'
  s.license = { :type => 'Apache-2.0', :file => '../../LICENSE' }
  s.authors = { 'native-shell contributors' => 'TODO@example.invalid' }
  s.source = { :git => 'https://example.invalid/TODO-native-shell.git', :tag => "v#{s.version}" }

  s.ios.deployment_target = '14.0'
  s.swift_versions = ['5.9']

  s.source_files = 'Sources/NativeShell/**/*.swift'
  s.dependency 'NativeShellShared', s.version.to_s
  s.vendored_frameworks = 'Frameworks/NativeShellFFI.xcframework'
  s.frameworks = 'UIKit', 'QuartzCore', 'ImageIO', 'Security', 'SystemConfiguration'
  s.libraries = 'c++'
end
