package dev.viselab.parity;

import com.mojang.brigadier.tree.ArgumentCommandNode;
import com.mojang.brigadier.tree.CommandNode;
import java.io.PrintWriter;
import java.nio.file.Files;
import java.nio.file.Path;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.Commands;
import net.minecraft.commands.synchronization.SuggestionProviders;

public final class VanillaCommandSuggestionsExporter {
    public static void main(String[] args) throws Exception {
        ParityData.init();
        Commands commands = new Commands(Commands.CommandSelection.ALL, Commands.createValidationContext(ParityData.lookup));
        try (PrintWriter out = new PrintWriter(Files.newBufferedWriter(Path.of(args[0])))) {
            walk(commands.getDispatcher().getRoot(), "", out);
        }
    }

    static void walk(CommandNode<CommandSourceStack> node, String path, PrintWriter out) {
        for (CommandNode<CommandSourceStack> child : node.getChildren()) {
            String childPath = path.isEmpty() ? child.getName() : path + " " + child.getName();
            if (child instanceof ArgumentCommandNode<CommandSourceStack, ?> arg && arg.getCustomSuggestions() != null) {
                out.println(childPath + "\t" + SuggestionProviders.getName(arg.getCustomSuggestions()));
            }
            walk(child, childPath, out);
        }
    }
}
