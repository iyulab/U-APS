using System;
using System.Windows;
using Microsoft.Extensions.DependencyInjection;
using Radzen;
using UAPS.Workbench.Services;

namespace UAPS.Workbench;

public partial class MainWindow : Window
{
    public MainWindow()
    {
        try
        {
            InitializeComponent();

            var services = new ServiceCollection();
            services.AddWpfBlazorWebView();

#if DEBUG
            services.AddBlazorWebViewDeveloperTools();
#endif

            // Register Radzen services
            services.AddScoped<DialogService>();
            services.AddScoped<NotificationService>();
            services.AddScoped<TooltipService>();
            services.AddScoped<ContextMenuService>();

            // Register application services
            services.AddSingleton<WorkbenchState>();
            services.AddSingleton<SchedulingService>();
            services.AddSingleton<FileService>();

            BlazorWebView.Services = services.BuildServiceProvider();
        }
        catch (Exception ex)
        {
            MessageBox.Show($"Initialization error: {ex.Message}\n\n{ex.StackTrace}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
            throw;
        }
    }
}
