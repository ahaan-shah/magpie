# Homebrew cask for Magpie. Lives in a tap repo, e.g. github.com/ahaan-shah/homebrew-tap
# as Casks/magpie-finance.rb, so users can run:
#   brew install --cask ahaan-shah/tap/magpie-finance
# After each release, bump `version` and `sha256` (from SHA256SUMS, the .zip line).
cask "magpie-finance" do
  version "0.2.3"
  sha256 "REPLACE_WITH_SHA256_OF_Magpie-macos-universal.zip"

  url "https://github.com/ahaan-shah/magpie/releases/download/v#{version}/Magpie-macos-universal.zip"
  name "Magpie"
  desc "Fast, beautiful, local-first personal finance tracker"
  homepage "https://github.com/ahaan-shah/magpie"

  depends_on macos: ">= :big_sur"

  app "Magpie.app"
  binary "#{appdir}/Magpie.app/Contents/MacOS/magpie"

  zap trash: [
    "~/Library/Application Support/dev.magpie.magpie",
    "~/Library/Application Support/magpie",
  ]
end
