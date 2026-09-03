using System.Diagnostics;
using System.IO.Compression;
using System.Net.Http.Headers;
using System.Reflection;
using System.Runtime.InteropServices;
using UAPS.CLI.Excel;
using UAPS.CLI.Json;
using UAPS.CLI.Simulation;
using UAPS.CLI.Tsv;
using UAPS.SDK.Client;
using UAPS.SDK.Interop;
using UAPS.SDK.Models;

namespace UAPS.CLI;

class Program
{
    private const string GitHubRepo = "iyulab/U-APS-releases";

    static async Task<int> Main(string[] args)
    {
        try
        {
            // Handle commands
            if (args.Length > 0)
            {
                switch (args[0].ToLower())
                {
                    case "update":
                        return await UpdateAsync();
                    case "version":
                    case "--version":
                    case "-v":
                        ShowVersion();
                        return 0;
                    case "help":
                    case "--help":
                    case "-h":
                        ShowUsage();
                        return 0;
                    case "rules":
                        ShowRules();
                        return 0;
                    case "simulate":
                    case "sim":
                        await NativeLoader.EnsureLoadedAsync();
                        return RunSimulation(args.Skip(1).ToArray());
                }
            }

            if (args.Length < 2)
            {
                ShowUsage();
                return 1;
            }

            // Ensure native library is available
            await NativeLoader.EnsureLoadedAsync();

            var inputPath = args[0];
            var outputPath = args[1];

            // '>' 구분자 처리 (samples/scenario-001/input.xlsx > output.xlsx)
            if (args.Length >= 3 && args[1] == ">")
            {
                outputPath = args[2];
            }

            // --rule 옵션 파싱
            string? dispatchingRule = null;
            string? tieBreaker = null;
            MultiLayerDispatchingConfig? strategyConfig = null;

            var ruleIndex = Array.FindIndex(args, a => a == "--rule" || a == "-r");
            if (ruleIndex >= 0 && ruleIndex + 1 < args.Length)
            {
                dispatchingRule = args[ruleIndex + 1].ToUpperInvariant();
                if (!DispatchingRules.IsValid(dispatchingRule))
                {
                    Console.Error.WriteLine($"Error: Invalid dispatching rule: {dispatchingRule}");
                    Console.Error.WriteLine($"Valid rules: {string.Join(", ", DispatchingRules.All)}");
                    return 1;
                }
            }

            var tieBreakerIndex = Array.FindIndex(args, a => a == "--tie-breaker" || a == "-t");
            if (tieBreakerIndex >= 0 && tieBreakerIndex + 1 < args.Length)
            {
                tieBreaker = args[tieBreakerIndex + 1].ToUpperInvariant();
                if (!DispatchingRules.IsValid(tieBreaker))
                {
                    Console.Error.WriteLine($"Error: Invalid tie-breaker rule: {tieBreaker}");
                    Console.Error.WriteLine($"Valid rules: {string.Join(", ", DispatchingRules.All)}");
                    return 1;
                }
            }

            // --strategy 옵션 파싱 (프리셋 기반 다계층 전략)
            var strategyIndex = Array.FindIndex(args, a => a == "--strategy" || a == "-S");
            if (strategyIndex >= 0 && strategyIndex + 1 < args.Length)
            {
                var strategyName = args[strategyIndex + 1];
                strategyConfig = DispatchingPresets.GetPreset(strategyName);
                if (strategyConfig == null)
                {
                    Console.Error.WriteLine($"Error: Invalid strategy preset: {strategyName}");
                    Console.Error.WriteLine($"Valid presets: {string.Join(", ", DispatchingPresets.AllPresets)}");
                    return 1;
                }
            }

            if (!File.Exists(inputPath))
            {
                Console.Error.WriteLine($"Error: Input file not found: {inputPath}");
                return 1;
            }

            Console.WriteLine("=== U-APS Scheduler ===");
            Console.WriteLine($"Input:  {inputPath}");
            Console.WriteLine($"Output: {outputPath}");
            if (strategyConfig != null)
            {
                Console.WriteLine($"Strategy: {args[strategyIndex + 1]} ({strategyConfig.Layers.Count} layers)");
            }
            else if (dispatchingRule != null)
            {
                Console.WriteLine($"Rule:   {dispatchingRule}" + (tieBreaker != null ? $" (tie-breaker: {tieBreaker})" : ""));
            }
            Console.WriteLine();

            // 1. 입력 파싱 (형식 자동 감지)
            Console.WriteLine("Parsing input file...");
            var inputExt = Path.GetExtension(inputPath).ToLowerInvariant();
            List<Job> jobs;
            List<Resource> resources;

            if (inputExt == ".json")
            {
                var parser = new JsonInputParser();
                (jobs, resources) = parser.Parse(inputPath);
            }
            else if (inputExt == ".tsv")
            {
                var parser = new TsvInputParser();
                (jobs, resources) = parser.ParseFile(inputPath);
            }
            else
            {
                var parser = new ExcelInputParser();
                (jobs, resources) = parser.Parse(inputPath);
            }

            Console.WriteLine($"  - Jobs: {jobs.Count}");
            Console.WriteLine($"  - Operations: {jobs.Sum(j => j.Operations.Count)}");
            Console.WriteLine($"  - Resources: {resources.Count}");
            Console.WriteLine();

            // 2. 스케줄링 실행
            Console.WriteLine("Running scheduler...");
            Schedule schedule;

            try
            {
                // Rust 엔진 시도
                var client = new SchedulerClient();
                var request = new ScheduleRequest { Jobs = jobs, Resources = resources };

                // Dispatching 설정 적용 (strategy 우선)
                if (strategyConfig != null)
                {
                    request.DispatchingConfig = new DispatchingConfig
                    {
                        PrimaryRule = strategyConfig.DefaultRule,
                        MultiLayer = strategyConfig
                    };
                }
                else if (dispatchingRule != null)
                {
                    request.DispatchingConfig = new DispatchingConfig
                    {
                        PrimaryRule = dispatchingRule,
                        TieBreaker = tieBreaker
                    };
                }

                var result = client.Schedule(request);

                if (!result.Success || result.Schedule == null)
                {
                    throw new Exception(result.Error ?? "Scheduling failed");
                }
                schedule = result.Schedule;
            }
            catch (DllNotFoundException)
            {
                // Rust DLL 없으면 Mock 스케줄러 사용
                Console.WriteLine("  (Using mock scheduler - Rust engine not available)");
                schedule = RunMockScheduler(jobs, resources);
            }

            Console.WriteLine($"  - Makespan: {schedule.MakespanMs / 60000.0:F1} minutes");
            Console.WriteLine($"  - Assignments: {schedule.Assignments.Count}");
            Console.WriteLine($"  - On Time: {(schedule.IsOnTime() ? "Yes" : "No")}");
            Console.WriteLine($"  - Violations: {schedule.Violations.Count}");
            Console.WriteLine();

            // 3. 출력 (형식 자동 감지)
            Console.WriteLine("Writing output file...");
            var outputExt = Path.GetExtension(outputPath).ToLowerInvariant();

            if (outputExt == ".json")
            {
                var writer = new JsonOutputWriter();
                writer.Write(schedule, jobs, outputPath);
            }
            else if (outputExt == ".tsv")
            {
                var writer = new TsvOutputWriter();
                writer.Write(outputPath, schedule);
            }
            else
            {
                var writer = new ExcelOutputWriter();
                writer.Write(schedule, jobs, outputPath);
            }

            Console.WriteLine();
            Console.WriteLine($"Schedule complete: {outputPath}");

            return 0;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"Error: {ex.Message}");
            return 1;
        }
    }

    static void ShowUsage()
    {
        Console.WriteLine("U-APS Scheduler CLI");
        Console.WriteLine();
        Console.WriteLine("Usage:");
        Console.WriteLine("  uaps <input> <output> [--rule <rule>]        Run scheduler with simple rule");
        Console.WriteLine("  uaps <input> <output> [--strategy <preset>]  Run with multi-layer strategy");
        Console.WriteLine("  uaps simulate <input>                        Interactive simulation");
        Console.WriteLine("  uaps simulate <input> --script <changes.json> --output <out.xlsx>");
        Console.WriteLine("                                               Batch simulation");
        Console.WriteLine("  uaps update                                  Update CLI and native library");
        Console.WriteLine("  uaps version                                 Show version");
        Console.WriteLine("  uaps help                                    Show this help");
        Console.WriteLine("  uaps rules                                   List available dispatching rules");
        Console.WriteLine();
        Console.WriteLine("Dispatching Rules (--rule/-r):");
        Console.WriteLine("  Time-based:  SPT (Shortest Processing Time), LPT (Longest Processing Time)");
        Console.WriteLine("               LWKR (Least Work Remaining), MWKR (Most Work Remaining)");
        Console.WriteLine("  Due date:    EDD (Earliest Due Date), MST (Minimum Slack Time)");
        Console.WriteLine("               CR (Critical Ratio), S/RO (Slack per Remaining Operations)");
        Console.WriteLine("  Queue/Load:  FIFO (First In First Out), WINQ (Work In Next Queue)");
        Console.WriteLine("               LPUL (Least Planned Utilization Level)");
        Console.WriteLine("  Priority:    PRIORITY (Job priority field)");
        Console.WriteLine();
        Console.WriteLine("Multi-Layer Strategies (--strategy/-S):");
        Console.WriteLine("  DUE_DATE_FOCUSED  EDD primary, SPT tie-breaker");
        Console.WriteLine("  THROUGHPUT        SPT only for maximum throughput");
        Console.WriteLine("  BALANCED          Critical Ratio with FIFO tie-breaker");
        Console.WriteLine("  URGENCY           Priority-based with EDD secondary");
        Console.WriteLine("  LOAD_BALANCING    LPUL with WINQ for load distribution");
        Console.WriteLine("  FAIR              Simple FIFO for fair queuing");
        Console.WriteLine("  ADAPTIVE          Dynamic rule switching based on conditions");
        Console.WriteLine();
        Console.WriteLine("Supported formats:");
        Console.WriteLine("  - Excel: .xlsx");
        Console.WriteLine("  - JSON:  .json");
        Console.WriteLine("  - TSV:   .tsv");
        Console.WriteLine();
        Console.WriteLine("Examples:");
        Console.WriteLine("  uaps input.xlsx output.xlsx");
        Console.WriteLine("  uaps input.xlsx output.xlsx --rule EDD");
        Console.WriteLine("  uaps input.xlsx output.xlsx --rule SPT --tie-breaker FIFO");
        Console.WriteLine("  uaps input.xlsx output.xlsx --strategy BALANCED");
        Console.WriteLine("  uaps input.xlsx output.xlsx --strategy ADAPTIVE");
        Console.WriteLine("  uaps simulate input.xlsx");
        Console.WriteLine("  uaps simulate input.xlsx --script changes.json --output result.xlsx");
        Console.WriteLine("  uaps update");
        Console.WriteLine("  uaps rules");
    }

    static int RunSimulation(string[] args)
    {
        if (args.Length == 0)
        {
            Console.Error.WriteLine("Error: Input file required for simulation");
            Console.Error.WriteLine("Usage: uaps simulate <input.xlsx> [--script <changes.json>] [--output <output.xlsx>]");
            return 1;
        }

        var inputPath = args[0];
        if (!File.Exists(inputPath))
        {
            Console.Error.WriteLine($"Error: Input file not found: {inputPath}");
            return 1;
        }

        var runner = new SimulationRunner();

        // Check for batch mode (--script flag)
        var scriptIndex = Array.FindIndex(args, a => a == "--script" || a == "-s");
        var outputIndex = Array.FindIndex(args, a => a == "--output" || a == "-o");

        if (scriptIndex >= 0 && scriptIndex + 1 < args.Length)
        {
            var changesPath = args[scriptIndex + 1];
            var outputPath = outputIndex >= 0 && outputIndex + 1 < args.Length
                ? args[outputIndex + 1]
                : Path.ChangeExtension(inputPath, ".result.xlsx");

            return runner.RunBatch(inputPath, changesPath, outputPath);
        }

        // Interactive mode
        return runner.RunInteractive(inputPath);
    }

    static void ShowVersion()
    {
        var version = Assembly.GetExecutingAssembly()
            .GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion ?? "unknown";
        Console.WriteLine($"uaps {version}");
    }

    static void ShowRules()
    {
        Console.WriteLine("=== Available Dispatching Rules ===");
        Console.WriteLine();
        Console.WriteLine("Time-based Rules:");
        Console.WriteLine("  SPT   - Shortest Processing Time");
        Console.WriteLine("          Prioritize jobs with shortest processing time first");
        Console.WriteLine("          Best for: Maximizing throughput, reducing flow time");
        Console.WriteLine();
        Console.WriteLine("  LPT   - Longest Processing Time");
        Console.WriteLine("          Prioritize jobs with longest processing time first");
        Console.WriteLine("          Best for: Balancing machine loads");
        Console.WriteLine();
        Console.WriteLine("  LWKR  - Least Work Remaining");
        Console.WriteLine("          Prioritize jobs with least remaining work");
        Console.WriteLine("          Best for: Getting jobs completed faster");
        Console.WriteLine();
        Console.WriteLine("  MWKR  - Most Work Remaining");
        Console.WriteLine("          Prioritize jobs with most remaining work");
        Console.WriteLine("          Best for: Starting complex jobs early");
        Console.WriteLine();
        Console.WriteLine("  WSPT  - Weighted Shortest Processing Time");
        Console.WriteLine("          Prioritize by weight/processing time ratio");
        Console.WriteLine("          Best for: Minimizing weighted completion time");
        Console.WriteLine();
        Console.WriteLine("Due Date Rules:");
        Console.WriteLine("  EDD   - Earliest Due Date");
        Console.WriteLine("          Prioritize jobs with earliest due dates");
        Console.WriteLine("          Best for: Minimizing maximum lateness");
        Console.WriteLine();
        Console.WriteLine("  MST   - Minimum Slack Time");
        Console.WriteLine("          Prioritize jobs with least buffer time");
        Console.WriteLine("          Best for: Meeting tight deadlines");
        Console.WriteLine();
        Console.WriteLine("  CR    - Critical Ratio");
        Console.WriteLine("          Prioritize based on (time to due) / (remaining work)");
        Console.WriteLine("          Best for: Balancing due dates with work remaining");
        Console.WriteLine();
        Console.WriteLine("  S/RO  - Slack per Remaining Operations");
        Console.WriteLine("          Prioritize by slack divided by operations left");
        Console.WriteLine("          Best for: Complex jobs with multiple operations");
        Console.WriteLine();
        Console.WriteLine("  ATC   - Apparent Tardiness Cost");
        Console.WriteLine("          Combines SPT with due date urgency (exp decay)");
        Console.WriteLine("          Best for: Minimizing weighted tardiness");
        Console.WriteLine();
        Console.WriteLine("Queue/Load Rules:");
        Console.WriteLine("  FIFO  - First In First Out");
        Console.WriteLine("          Process jobs in arrival order");
        Console.WriteLine("          Best for: Fair scheduling, simple cases");
        Console.WriteLine();
        Console.WriteLine("  WINQ  - Work In Next Queue");
        Console.WriteLine("          Prefer jobs heading to less loaded resources");
        Console.WriteLine("          Best for: Reducing downstream bottlenecks");
        Console.WriteLine();
        Console.WriteLine("  LPUL  - Least Planned Utilization Level");
        Console.WriteLine("          Use resources with lowest planned utilization");
        Console.WriteLine("          Best for: Balancing resource loads");
        Console.WriteLine();
        Console.WriteLine("Priority-based:");
        Console.WriteLine("  PRIORITY - Job Priority Field");
        Console.WriteLine("             Use the priority field from job data");
        Console.WriteLine("             Best for: Business priority-driven scheduling");
        Console.WriteLine();
        Console.WriteLine("Usage:");
        Console.WriteLine("  uaps input.xlsx output.xlsx --rule EDD");
        Console.WriteLine("  uaps input.xlsx output.xlsx --rule SPT --tie-breaker FIFO");
    }

    static async Task<int> UpdateAsync()
    {
        Console.WriteLine("=== UAPS Update ===");
        Console.WriteLine();

        try
        {
            using var httpClient = new HttpClient();
            httpClient.DefaultRequestHeaders.UserAgent.Add(new ProductInfoHeaderValue("UAPS-CLI", "1.0"));

            // Get latest release info
            Console.WriteLine("Checking for updates...");
            var releaseUrl = $"https://api.github.com/repos/{GitHubRepo}/releases/latest";
            var releaseJson = await httpClient.GetStringAsync(releaseUrl);

            // Simple JSON parsing for tag_name
            var tagMatch = System.Text.RegularExpressions.Regex.Match(releaseJson, "\"tag_name\"\\s*:\\s*\"([^\"]+)\"");
            if (!tagMatch.Success)
            {
                Console.Error.WriteLine("Failed to get latest version");
                return 1;
            }

            var latestVersion = tagMatch.Groups[1].Value.TrimStart('v');
            var currentVersion = Assembly.GetExecutingAssembly()
                .GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion ?? "0.0.0";

            // Remove any +metadata from version
            currentVersion = currentVersion.Split('+')[0];

            Console.WriteLine($"Current version: {currentVersion}");
            Console.WriteLine($"Latest version:  {latestVersion}");
            Console.WriteLine();

            if (currentVersion == latestVersion)
            {
                Console.WriteLine("Already up to date!");
                Console.WriteLine();

                // Still update native library
                Console.WriteLine("Updating native library...");
                await UpdateNativeLibraryAsync(httpClient, latestVersion);
                return 0;
            }

            // Update CLI
            Console.WriteLine("Updating CLI...");
            await UpdateCliAsync(httpClient, latestVersion);

            // Update native library
            Console.WriteLine("Updating native library...");
            await UpdateNativeLibraryAsync(httpClient, latestVersion);

            Console.WriteLine();
            Console.WriteLine($"Updated to version {latestVersion}!");
            Console.WriteLine("Please restart the CLI to use the new version.");

            return 0;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"Update failed: {ex.Message}");
            return 1;
        }
    }

    static async Task UpdateCliAsync(HttpClient httpClient, string version)
    {
        var rid = NativeLoader.GetRuntimeIdentifier();
        var ext = RuntimeInformation.IsOSPlatform(OSPlatform.Windows) ? "zip" : "tar.gz";
        var fileName = $"uaps-cli-{rid}.{ext}";
        var downloadUrl = $"https://github.com/{GitHubRepo}/releases/download/v{version}/{fileName}";

        Console.WriteLine($"  Downloading {fileName}...");

        var tempFile = Path.GetTempFileName();
        try
        {
            using var response = await httpClient.GetAsync(downloadUrl, HttpCompletionOption.ResponseHeadersRead);
            response.EnsureSuccessStatusCode();

            await using (var fileStream = new FileStream(tempFile, FileMode.Create))
            {
                await response.Content.CopyToAsync(fileStream);
            }

            // Extract to temp directory
            var tempDir = Path.Combine(Path.GetTempPath(), $"uaps-update-{Guid.NewGuid()}");
            Directory.CreateDirectory(tempDir);

            if (ext == "zip")
            {
                ZipFile.ExtractToDirectory(tempFile, tempDir);
            }
            else
            {
                // Use tar for Unix
                var startInfo = new ProcessStartInfo
                {
                    FileName = "tar",
                    Arguments = $"-xzf \"{tempFile}\" -C \"{tempDir}\"",
                    UseShellExecute = false,
                    CreateNoWindow = true
                };
                using var process = Process.Start(startInfo);
                process?.WaitForExit();
            }

            // Get current executable location
            var currentExe = Environment.ProcessPath ?? Assembly.GetExecutingAssembly().Location;
            var installDir = Path.GetDirectoryName(currentExe)!;

            // Copy new files (skip the currently running executable on Windows)
            foreach (var file in Directory.GetFiles(tempDir))
            {
                var destFile = Path.Combine(installDir, Path.GetFileName(file));

                // On Windows, rename current exe first
                if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows) &&
                    destFile.Equals(currentExe, StringComparison.OrdinalIgnoreCase))
                {
                    var oldExe = currentExe + ".old";
                    if (File.Exists(oldExe)) File.Delete(oldExe);
                    File.Move(currentExe, oldExe);
                }

                File.Copy(file, destFile, overwrite: true);
            }

            // Cleanup
            Directory.Delete(tempDir, recursive: true);
            Console.WriteLine("  CLI updated.");
        }
        finally
        {
            if (File.Exists(tempFile)) File.Delete(tempFile);
        }
    }

    static async Task UpdateNativeLibraryAsync(HttpClient httpClient, string version)
    {
        var rid = NativeLoader.GetRuntimeIdentifier();
        var fileName = rid switch
        {
            "win-x64" or "win-x86" or "win-arm64" => $"uaps_engine-{rid}.dll",
            "linux-x64" or "linux-arm64" => $"uaps_engine-{rid}.so",
            "osx-x64" or "osx-arm64" => $"uaps_engine-{rid}.dylib",
            _ => throw new PlatformNotSupportedException($"Unsupported: {rid}")
        };

        var downloadUrl = $"https://github.com/{GitHubRepo}/releases/download/v{version}/{fileName}";
        var targetPath = NativeLoader.GetLibraryPath();

        Console.WriteLine($"  Downloading {fileName}...");

        using var response = await httpClient.GetAsync(downloadUrl, HttpCompletionOption.ResponseHeadersRead);
        response.EnsureSuccessStatusCode();

        var tempPath = targetPath + ".tmp";
        await using (var fileStream = new FileStream(tempPath, FileMode.Create))
        {
            await response.Content.CopyToAsync(fileStream);
        }

        File.Move(tempPath, targetPath, overwrite: true);

        // Set executable permission on Unix
        if (!RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
        {
            File.SetUnixFileMode(targetPath, UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute |
                                              UnixFileMode.GroupRead | UnixFileMode.GroupExecute |
                                              UnixFileMode.OtherRead | UnixFileMode.OtherExecute);
        }

        Console.WriteLine($"  Native library updated: {targetPath}");
    }

    /// <summary>
    /// Mock 스케줄러 - Rust 엔진 없을 때 사용
    /// </summary>
    static Schedule RunMockScheduler(List<Job> jobs, List<Resource> resources)
    {
        var assignments = new List<Assignment>();
        var violations = new List<Violation>();

        // 자원 가용 시간 추적
        var resourceAvailability = resources.ToDictionary(r => r.Id, r => 0L);

        // 우선순위 순으로 정렬
        var sortedJobs = jobs.OrderBy(j => j.Priority).ToList();

        foreach (var job in sortedJobs)
        {
            var jobStartTime = 0L;

            foreach (var op in job.Operations.OrderBy(o => o.Sequence))
            {
                // 사용 가능한 자원 찾기 (RequiredResources에서 Equipment 타입 후보 추출)
                var equipmentCandidates = op.RequiredResources
                    .Where(r => r.ResourceType == ResourceType.Equipment)
                    .SelectMany(r => r.Candidates)
                    .ToList();

                var availableResource = equipmentCandidates
                    .Where(id => resourceAvailability.ContainsKey(id))
                    .OrderBy(id => resourceAvailability[id])
                    .FirstOrDefault();

                if (availableResource == null && resources.Count > 0)
                {
                    availableResource = resources[0].Id;
                }

                availableResource ??= "DEFAULT";

                // 시작 시간 계산
                var resourceReady = resourceAvailability.GetValueOrDefault(availableResource, 0);
                var startTime = Math.Max(jobStartTime, resourceReady);

                // 종료 시간 계산
                var duration = op.Time.SetupMs + op.Time.ProcessMs + op.Time.WaitMs;
                var endTime = startTime + duration;

                assignments.Add(new Assignment
                {
                    OperationId = op.Id,
                    ResourceId = availableResource,
                    StartMs = startTime,
                    EndMs = endTime
                });

                // 자원 가용 시간 업데이트
                if (resourceAvailability.ContainsKey(availableResource))
                {
                    resourceAvailability[availableResource] = endTime;
                }

                jobStartTime = endTime;
            }

            // 납기 위반 확인
            if (job.DueDate.HasValue)
            {
                var completionTime = jobStartTime;
                var dueTimeMs = (long)(job.DueDate.Value - DateTime.UnixEpoch).TotalMilliseconds;

                if (completionTime > dueTimeMs && dueTimeMs > 0)
                {
                    violations.Add(new Violation
                    {
                        ConstraintType = "DueDate",
                        TargetId = job.Id,
                        Amount = completionTime - dueTimeMs,
                        Description = $"Job {job.Id} completes {(completionTime - dueTimeMs) / 60000.0:F1} minutes late"
                    });
                }
            }
        }

        var makespan = assignments.Count > 0 ? assignments.Max(a => a.EndMs) : 0;

        return new Schedule
        {
            Assignments = assignments,
            Violations = violations,
            MakespanMs = makespan
        };
    }
}
