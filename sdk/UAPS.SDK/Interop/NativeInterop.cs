using System.Reflection;
using System.Runtime.InteropServices;

namespace UAPS.SDK.Interop;

/// <summary>
/// Rust 엔진 P/Invoke 래퍼
/// </summary>
internal static partial class NativeInterop
{
    private const string DllName = "uaps_engine";

    static NativeInterop()
    {
        // Set up custom resolver for native library
        NativeLibrary.SetDllImportResolver(typeof(NativeInterop).Assembly, DllImportResolver);
    }

    private static IntPtr DllImportResolver(string libraryName, Assembly assembly, DllImportSearchPath? searchPath)
    {
        if (libraryName != DllName)
            return IntPtr.Zero;

        // Try loading from standard paths first
        if (NativeLibrary.TryLoad(libraryName, assembly, searchPath, out var handle))
            return handle;

        // Try loading from NativeLoader path
        var customPath = NativeLoader.GetLibraryPath();
        if (File.Exists(customPath) && NativeLibrary.TryLoad(customPath, out handle))
            return handle;

        // Library not found - caller should use NativeLoader.EnsureLoaded() first
        return IntPtr.Zero;
    }

    /// <summary>
    /// 스케줄링 실행
    /// </summary>
    /// <param name="requestJson">JSON 요청</param>
    /// <param name="resultPtr">결과 포인터</param>
    /// <returns>0: 성공, 음수: 에러</returns>
    [LibraryImport(DllName, StringMarshalling = StringMarshalling.Utf8)]
    public static partial int uaps_schedule(
        string requestJson,
        out IntPtr resultPtr);

    /// <summary>
    /// 문자열 메모리 해제
    /// </summary>
    [LibraryImport(DllName)]
    public static partial void uaps_free_string(IntPtr ptr);

    /// <summary>
    /// 엔진 버전
    /// </summary>
    [LibraryImport(DllName)]
    public static partial IntPtr uaps_version();
}
