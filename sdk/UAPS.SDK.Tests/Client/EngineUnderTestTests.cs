using System.Runtime.InteropServices;
using AwesomeAssertions;
using UAPS.SDK.Client;
using Xunit;

namespace UAPS.SDK.Tests.Client;

/// <summary>
/// The native tests must run against the engine built from this repository,
/// not the SDK's download cache (a published engine) — otherwise a change on
/// one side of the SDK/engine contract is tested against the other side's old
/// version and passes.
/// </summary>
public class EngineUnderTestTests
{
    [Fact]
    public void Engine_ResolvesNextToTheTests_NotFromTheDownloadCache()
    {
        var resolved = NativeLibrary.TryLoad("uaps_engine", typeof(SchedulerClient).Assembly, null, out var handle);
        if (resolved)
        {
            NativeLibrary.Free(handle);
        }

        resolved.Should().BeTrue(
            "the engine library should be copied next to the tests from `cargo build` " +
            "(engine/, or CARGO_TARGET_DIR) or staged under UAPS.SDK/runtimes/ — " +
            "without it the SDK loads its download cache, a published engine");
    }
}
