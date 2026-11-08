# Homebrew cask template for Kraken. scripts/cask.sh fills in @VERSION@, @SHA256@ and
# @REPO@ and writes the result to target/kraken.rb; copy that into a tap repo
# (e.g. <owner>/homebrew-tap at Casks/kraken.rb) to publish.
cask "kraken" do
  version "@VERSION@"
  sha256 "@SHA256@"

  url "https://github.com/@REPO@/releases/download/v#{version}/Kraken-#{version}.dmg"
  name "Kraken"
  desc "GPU-accelerated terminal emulator"
  homepage "https://github.com/@REPO@"

  livecheck do
    url :url
    strategy :github_latest
  end

  # Matches LSMinimumSystemVersion (13.0) in Info.plist.
  depends_on macos: ">= :ventura"

  app "Kraken.app"
  binary "#{appdir}/Kraken.app/Contents/MacOS/kraken"

  zap trash: [
    "~/.config/kraken",
    "~/Library/Application Support/Kraken",
    "~/Library/Logs/Kraken",
    "~/Library/Preferences/dev.kraken.terminal.plist",
  ]
end
