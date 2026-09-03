using UAPS.SDK.Client;
using UAPS.SDK.Models;
using UAPS.SDK.Analytics;

namespace UAPS.Workbench.Services;

/// <summary>
/// Service for executing scheduling operations
/// </summary>
public class SchedulingService : IDisposable
{
    private readonly SchedulerClient _client;
    private bool _disposed;

    public SchedulingService()
    {
        _client = new SchedulerClient();
    }

    /// <summary>
    /// Get engine version
    /// </summary>
    public string GetVersion()
    {
        try
        {
            return _client.GetVersion();
        }
        catch
        {
            return "N/A";
        }
    }

    /// <summary>
    /// Execute scheduling with the given request
    /// </summary>
    public ScheduleResult Execute(ScheduleRequest request)
    {
        return _client.Schedule(request);
    }

    /// <summary>
    /// Execute scheduling and calculate KPIs
    /// </summary>
    public (ScheduleResult Result, KpiDashboard? Kpis) ExecuteWithKpis(ScheduleRequest request)
    {
        var result = _client.Schedule(request);

        KpiDashboard? kpis = null;
        if (result.Success && result.Schedule != null)
        {
            var calculator = new KpiCalculator(result.Schedule, request.Jobs, request.Resources);
            kpis = calculator.Calculate();
        }

        return (result, kpis);
    }

    /// <summary>
    /// Calculate KPIs for an existing result
    /// </summary>
    public KpiDashboard CalculateKpis(Schedule schedule, List<Job> jobs, List<Resource> resources)
    {
        var calculator = new KpiCalculator(schedule, jobs, resources);
        return calculator.Calculate();
    }

    /// <summary>
    /// Run a What-If scenario
    /// </summary>
    public WhatIfResult RunWhatIf(
        ScheduleRequest baseRequest,
        ScheduleResult baseResult,
        WhatIfScenario scenario)
    {
        var simulator = new WhatIfSimulator(_client);
        return simulator.RunScenario(baseRequest, baseResult, scenario);
    }

    /// <summary>
    /// Run multiple What-If scenarios
    /// </summary>
    public List<WhatIfResult> RunWhatIfScenarios(
        ScheduleRequest baseRequest,
        ScheduleResult baseResult,
        IEnumerable<WhatIfScenario> scenarios)
    {
        var simulator = new WhatIfSimulator(_client);
        return simulator.RunScenarios(baseRequest, baseResult, scenarios);
    }

    public void Dispose()
    {
        if (!_disposed)
        {
            _client.Dispose();
            _disposed = true;
            GC.SuppressFinalize(this);
        }
    }
}
