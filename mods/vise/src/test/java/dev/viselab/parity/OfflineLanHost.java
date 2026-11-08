package dev.viselab.parity;

import net.minecraft.client.Minecraft;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.server.MinecraftServer;

public final class OfflineLanHost {
    public static void main(String[] args) throws Exception {
        int port = Integer.getInteger("vise.lanPort", 0);
        Thread watcher = new Thread(() -> {
            boolean published = false;
            while (!published) {
                try {
                    Thread.sleep(500);
                    Minecraft mc = Minecraft.getInstance();
                    IntegratedServer server = mc == null ? null : mc.getSingleplayerServer();
                    if (server == null || mc.player == null || !server.isReady()) continue;
                    server.setUsesAuthentication(false);
                    mc.execute(() -> {
                        boolean ok = server.publishServer(MinecraftServer.MultiplayerScope.LAN, true, port);
                        System.out.println("[OfflineLanHost] published=" + ok + " port=" + server.getPort());
                    });
                    published = true;
                } catch (Throwable t) {
                }
            }
        }, "vise-offline-lan");
        watcher.setDaemon(true);
        watcher.start();
        net.minecraft.client.main.Main.main(args);
    }
}
