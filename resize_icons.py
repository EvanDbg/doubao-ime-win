from PIL import Image
import os

def resize_icons(input_dir, output_dir, size=48):
    """Resize icons to specified size"""
    os.makedirs(output_dir, exist_ok=True)
    
    icons = ["icon_idle.png", "icon_recording.png", "icon_processing.png"]
    
    for icon_name in icons:
        input_path = os.path.join(input_dir, icon_name)
        output_path = os.path.join(output_dir, icon_name)
        
        if os.path.exists(input_path):
            img = Image.open(input_path)
            # Resize with high quality
            img = img.resize((size, size), Image.Resampling.LANCZOS)
            img.save(output_path, "PNG")
            print(f"Resized: {icon_name} -> {size}x{size}")
        else:
            print(f"Not found: {input_path}")

if __name__ == "__main__":
    resize_icons("assets", "assets", 48)
    print("Done!")
