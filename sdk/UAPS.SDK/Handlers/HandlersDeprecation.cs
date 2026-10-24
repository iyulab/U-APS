namespace UAPS.SDK.Handlers;

/// <summary>
/// Why the calendar handler surface in this namespace is deprecated (kept for compatibility only).
/// </summary>
internal static class HandlersDeprecation
{
    public const string Message = "Not used by the engine: calendars reach the engine through UAPS.SDK.Models.Calendar (shifts, breaks, date exceptions, time zone). This surface reads dates on the machine's local time zone. Kept for compatibility only.";
}
