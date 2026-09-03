using System.Net.Http.Headers;
using System.Reflection;
using System.Runtime.InteropServices;

namespace UAPS.SDK.Interop;

/// <summary>
/// Native library loader. UAPS.SDK bundles the engine binary for each supported
/// RID under the package's runtimes/ folder (see UAPS.SDK.csproj), so a normal
/// `dotnet add package UAPS.SDK` + build/publish already places it where the
/// runtime's standard native-library resolution finds it — the methods here
/// are then a fast no-op. The GitHub Releases download is a fallback for the
/// remaining case: an RID this package doesn't bundle a binary for.
/// </summary>
public static class NativeLoader
{
    private const string GitHubRepo = "iyulab/U-APS-releases";
    private const string LibraryBaseName = "uaps_engine";

    private static readonly object _lock = new();
    private static bool _initialized;
    private static string? _libraryPath;

    /// <summary>
    /// Ensure native library is available. Downloads if not present.
    /// </summary>
    /// <param name="version">Specific version to download (e.g., "0.1.0"). If null, uses latest.</param>
    /// <param name="cancellationToken">Cancellation token</param>
    public static async Task EnsureLoadedAsync(string? version = null, CancellationToken cancellationToken = default)
    {
        if (_initialized) return;

        lock (_lock)
        {
            if (_initialized) return;

            if (TryStandardResolution())
            {
                _initialized = true;
                return;
            }

            var libraryPath = GetLibraryPath();
            if (File.Exists(libraryPath))
            {
                _libraryPath = libraryPath;
                _initialized = true;
                return;
            }
        }

        // Download outside lock to avoid blocking
        await DownloadLibraryAsync(version, cancellationToken);

        lock (_lock)
        {
            _libraryPath = GetLibraryPath();
            _initialized = true;
        }
    }

    /// <summary>
    /// Ensure native library is available (synchronous version)
    /// </summary>
    public static void EnsureLoaded(string? version = null)
    {
        if (_initialized) return;

        lock (_lock)
        {
            if (_initialized) return;

            if (TryStandardResolution())
            {
                _initialized = true;
                return;
            }

            var libraryPath = GetLibraryPath();
            if (File.Exists(libraryPath))
            {
                _libraryPath = libraryPath;
                _initialized = true;
                return;
            }
        }

        // Download synchronously
        DownloadLibraryAsync(version, CancellationToken.None).GetAwaiter().GetResult();

        lock (_lock)
        {
            _libraryPath = GetLibraryPath();
            _initialized = true;
        }
    }

    /// <summary>
    /// Checks whether the engine binary resolves via the runtime's standard native-library
    /// search (same mechanism <see cref="NativeInterop"/>'s DllImportResolver tries first),
    /// which succeeds once a package-bundled runtimes/{rid}/native/ asset has been copied to
    /// the app's output by a normal build/publish. Caller must hold <see cref="_lock"/>.
    /// </summary>
    private static bool TryStandardResolution()
    {
        if (!NativeLibrary.TryLoad(LibraryBaseName, typeof(NativeLoader).Assembly, null, out var handle))
            return false;

        NativeLibrary.Free(handle);
        return true;
    }

    /// <summary>
    /// Get the expected library path for current platform
    /// </summary>
    public static string GetLibraryPath()
    {
        var baseDir = GetLibraryDirectory();
        var fileName = GetLibraryFileName();
        return Path.Combine(baseDir, fileName);
    }

    /// <summary>
    /// Get the runtime identifier for current platform
    /// </summary>
    public static string GetRuntimeIdentifier()
    {
        var arch = RuntimeInformation.ProcessArchitecture switch
        {
            Architecture.X64 => "x64",
            Architecture.Arm64 => "arm64",
            Architecture.X86 => "x86",
            _ => throw new PlatformNotSupportedException($"Unsupported architecture: {RuntimeInformation.ProcessArchitecture}")
        };

        if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
            return $"win-{arch}";
        if (RuntimeInformation.IsOSPlatform(OSPlatform.Linux))
            return $"linux-{arch}";
        if (RuntimeInformation.IsOSPlatform(OSPlatform.OSX))
            return $"osx-{arch}";

        throw new PlatformNotSupportedException($"Unsupported OS: {RuntimeInformation.OSDescription}");
    }

    private static string GetLibraryDirectory()
    {
        // Use local app data for downloaded libraries
        var appData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        var uapsDir = Path.Combine(appData, "UAPS", "native");
        Directory.CreateDirectory(uapsDir);
        return uapsDir;
    }

    private static string GetLibraryFileName()
    {
        if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
            return $"{LibraryBaseName}.dll";
        if (RuntimeInformation.IsOSPlatform(OSPlatform.Linux))
            return $"lib{LibraryBaseName}.so";
        if (RuntimeInformation.IsOSPlatform(OSPlatform.OSX))
            return $"lib{LibraryBaseName}.dylib";

        throw new PlatformNotSupportedException();
    }

    private static async Task DownloadLibraryAsync(string? version, CancellationToken cancellationToken)
    {
        var rid = GetRuntimeIdentifier();
        var fileName = GetDownloadFileName(rid);
        var url = GetDownloadUrl(version, fileName);
        var targetPath = GetLibraryPath();

        using var httpClient = new HttpClient();
        httpClient.DefaultRequestHeaders.UserAgent.Add(new ProductInfoHeaderValue("UAPS-SDK", GetSdkVersion()));

        Console.WriteLine($"Downloading UAPS engine for {rid}...");

        try
        {
            using var response = await httpClient.GetAsync(url, HttpCompletionOption.ResponseHeadersRead, cancellationToken);
            response.EnsureSuccessStatusCode();

            var tempPath = targetPath + ".tmp";
            await using (var fileStream = new FileStream(tempPath, FileMode.Create, FileAccess.Write, FileShare.None))
            {
                await response.Content.CopyToAsync(fileStream, cancellationToken);
            }

            // Atomic move
            File.Move(tempPath, targetPath, overwrite: true);

            // Set executable permission on Unix
            if (!RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
            {
                File.SetUnixFileMode(targetPath, UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute |
                                                  UnixFileMode.GroupRead | UnixFileMode.GroupExecute |
                                                  UnixFileMode.OtherRead | UnixFileMode.OtherExecute);
            }

            Console.WriteLine($"Downloaded to: {targetPath}");
        }
        catch (HttpRequestException ex)
        {
            throw new InvalidOperationException(
                $"Failed to download native library from {url}. " +
                $"Please download manually from https://github.com/{GitHubRepo}/releases and place in {GetLibraryDirectory()}",
                ex);
        }
    }

    private static string GetDownloadFileName(string rid)
    {
        return rid switch
        {
            "win-x64" or "win-x86" or "win-arm64" => $"{LibraryBaseName}-{rid}.dll",
            "linux-x64" or "linux-arm64" => $"{LibraryBaseName}-{rid}.so",
            "osx-x64" or "osx-arm64" => $"{LibraryBaseName}-{rid}.dylib",
            _ => throw new PlatformNotSupportedException($"Unsupported runtime identifier: {rid}")
        };
    }

    private static string GetDownloadUrl(string? version, string fileName)
    {
        if (string.IsNullOrEmpty(version))
        {
            return $"https://github.com/{GitHubRepo}/releases/latest/download/{fileName}";
        }
        return $"https://github.com/{GitHubRepo}/releases/download/v{version}/{fileName}";
    }

    private static string GetSdkVersion()
    {
        return Assembly.GetExecutingAssembly()
            .GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion
            ?? "1.0.0";
    }
}
